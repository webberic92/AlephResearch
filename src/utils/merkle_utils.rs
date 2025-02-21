use base64::{engine::general_purpose, Engine};
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};
use tracing::{ error, info};

use crate::structs::{dag::ReconstructedUnit, requests::{DagUnit, Transaction}};


pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    if hashes.is_empty() {
        return vec![0; 32]; // Standard default empty Merkle root
    }

    let mut current_level = hashes.to_vec();
    
    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        
        for chunk in current_level.chunks(2) {
            let mut combined = chunk[0].clone();
            
            // If odd-sized level, duplicate the last element
            if chunk.len() > 1 {
                combined.extend(&chunk[1]);
            } else {
                combined.extend(&chunk[0]); // Duplication instead of zero-padding
            }

            next_level.push(Sha256::digest(&combined).to_vec());
        }
        
        current_level = next_level;
    }

    current_level[0].clone()
}




pub fn compute_merkle_branch(hashes: &[Vec<u8>], index: usize) -> Vec<Vec<u8>> {
    let mut branch = vec![];
    let mut current_index = index;
    let mut current_level = hashes.to_vec();

    while current_level.len() > 1 {
        let sibling_index = if current_index % 2 == 0 {
            current_index + 1
        } else {
            current_index - 1
        };

        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
        } else {
            branch.push(vec![0; 32]); // Padding for missing sibling
        }

        // debug!(
        //     "Branch Level {}: Current Index = {}, Sibling Index = {}, Combined Hash = {:?}",
        //     current_level.len(),
        //     current_index,
        //     sibling_index,
        //     branch.last().unwrap()
        // );

        current_index /= 2;
        current_level = current_level
            .chunks(2)
            .map(|pair| {
                let mut combined = pair[0].clone();
                if pair.len() > 1 {
                    combined.extend(&pair[1]);
                } else {
                    combined.extend(vec![0; 32]); // Padding for odd-sized levels
                }
                Sha256::digest(&combined).to_vec()
            })
            .collect();
    }

    // debug!("Computed Merkle branch for index {}: {:?}", index, branch);
    branch
}



pub fn validate_merkle_branch(
    leaf: &[u8],        // The leaf (shard hash)
    proof: &[Vec<u8>],  // The proof path (Merkle branch)
    index: usize,       // Index of the leaf in the original tree
    expected_root: &[u8] // Expected Merkle root
) -> bool {
    let mut current_hash = leaf.to_vec();  // Start with the **actual leaf hash**
    let mut current_index = index;

    for sibling_hash in proof {
        let combined = if current_index % 2 == 0 {
            [current_hash.clone(), sibling_hash.clone()].concat()
        } else {
            [sibling_hash.clone(), current_hash.clone()].concat()
        };

        current_hash = Sha256::digest(&combined).to_vec();
        current_index /= 2;
    }

    // info!(
    //     "Validation result: Final hash = {:?}, Expected root = {:?}",
    //     current_hash, expected_root
    // );

    current_hash == expected_root
}



/**
 * **Reconstructs a DAG Unit from received transactions**
 * - Extracts transactions from decoded shards.
 * - Computes a separate Merkle root per transaction.
 * - Generates a unique unit ID for DAG tracking.
 */
pub fn reconstruct_unit(
    transactions: &[Transaction],  // ✅ Directly take the decoded transactions
    round_id: u64,
    parent_units: Vec<String>, // ✅ Already stored as parent unit IDs
    proposer_node: usize, // ✅ We need to pass the proposing node ID
) -> Result<DagUnit, String> {
    if transactions.is_empty() {
        return Err("Reconstruction failed: No transactions provided".to_string());
    }

    // Compute separate Merkle roots for each transaction
    let merkle_roots: Vec<Vec<u8>> = transactions.iter()
        .map(|tx| {
            let shard_hashes: Vec<Vec<u8>> = tx.shards.iter()
                .map(|shard| Sha256::digest(shard.as_bytes()).to_vec())
                .collect();
            compute_merkle_root(&shard_hashes)
        })
        .collect();

    // ✅ Log computed Merkle roots
    // info!("Computed Merkle roots for reconstructed unit: {:?}", merkle_roots);

    // Generate a unique unit ID
    let unit_id = format!("U{}-{}", round_id, proposer_node);

    // Create the DagUnit object
    Ok(DagUnit {
        unit_id,
        proposer_node,
        round: round_id,
        transactions: transactions.to_vec(),  // ✅ Store multiple transactions
        parent_units,
        merkle_root: merkle_roots.concat(), // ✅ Flatten all transaction Merkle roots
        finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    })
}




pub fn split_into_shards(transaction_data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
    let shard_size = transaction_data.len() / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    // info!("Shards split into {} parts: {:?}", data_shards, shards);
    shards
}


/// Validate shard sizes
pub fn validate_shard_sizes(shards: &[Vec<u8>], transaction_size: usize) -> Result<(), String> {
    let total_size: usize = shards.iter().map(|shard| shard.len()).sum();
    if total_size != transaction_size {
        let error_message = format!(
            "Shard size validation failed. Total shard size: {}, Expected transaction size: {}",
            total_size, transaction_size
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    // info!(
    //     "Shard size validation successful. Total shard size: {} matches transaction size: {}",
    //     total_size, transaction_size
    // );
    Ok(())
}


pub fn interpolate_shares(decoded_shards: &[Vec<u8>], round_id: u64) -> Result<Vec<Vec<u8>>, String> {
    // ✅ Handle round 1: No interpolation required
    if round_id == 1 {
        info!("Round 1 detected: Skipping interpolation, returning provided shards.");
        return Ok(decoded_shards.to_vec());
    }

    // Check if we have enough shards
    if decoded_shards.is_empty() {
        return Err("Interpolation failed: No available shards".to_string());
    }

    let total_shards = decoded_shards.len();
    let data_shards = (total_shards + 1) / 2; // Assumes f+1 shards for reconstruction
    let parity_shards = total_shards - data_shards;

    if data_shards < 1 {
        return Err(format!(
            "Interpolation failed: Not enough data shards ({}). Requires at least 1.",
            data_shards
        ));
    }

    // Initialize Reed-Solomon erasure coding
    let r = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| format!("Failed to create Reed-Solomon codec: {:?}", e))?;

    // Prepare shard buffer with None for missing shares
    let mut shard_buffer: Vec<Option<Vec<u8>>> = decoded_shards.iter().map(|s| Some(s.clone())).collect();
    shard_buffer.resize(data_shards + parity_shards, None);

    // Attempt to recover missing shares
    r.reconstruct(&mut shard_buffer)
        .map_err(|e| format!("Failed to interpolate shares: {:?}", e))?;

    // Convert buffer to final result, filtering out any None values
    let recovered_shards: Vec<Vec<u8>> = shard_buffer
        .into_iter()
        .filter_map(|s| s)
        .collect();

    if recovered_shards.len() < data_shards + parity_shards {
        return Err("Interpolation failed: Not all shares were recovered".to_string());
    }

    info!("Interpolated missing shares successfully.");
    Ok(recovered_shards)
}
