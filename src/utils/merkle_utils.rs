use sha2::{Digest, Sha256};
use tracing::error;

use crate::structs:: requests::{DagUnit, Transaction};


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




pub fn compute_merkle_branch(hashes: &[Vec<u8>], mut index: usize) -> Vec<Vec<u8>> {
    let mut branch = vec![];
    let mut current_level = hashes.to_vec();

    while current_level.len() > 1 {
        let is_right_node = index % 2 == 1;
        let sibling_index = if is_right_node { index - 1 } else { index + 1 };

        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
        } else {
            // 🔁 For odd-sized levels, include duplicate of current hash (not sibling)
            branch.push(current_level[index].clone());
        }

        index /= 2;
        current_level = current_level
            .chunks(2)
            .map(|pair| {
                let left = &pair[0];
                let right = if pair.len() == 2 { &pair[1] } else { &pair[0] };
                Sha256::digest([left.clone(), right.clone()].concat()).to_vec()
            })
            .collect();
    }

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
    transactions: &[Transaction],
    round_id: u64,
    parent_units: Vec<String>, // ✅ use unit_id strings like "U1-1"
    proposer_node: usize,
    batch_merkle_root: Vec<u8>,
) -> Result<DagUnit, String> {
    if transactions.is_empty() {
        return Err("Reconstruction failed: No transactions provided".to_string());
    }

    // info!(
    //     "Reconstructing unit for round {} from {} transactions, parent count = {}, proposer = {}",
    //     round_id, transactions.len(), parent_units.len(), proposer_node
    // );


    let reconstructed_transactions: Vec<Transaction> = transactions.iter().cloned().collect();
    let unit_id = format!("U{}-{}", round_id, proposer_node);

    // info!("✅ Reconstructed unit {} with Merkle root {}", unit_id, hex::encode(&batch_merkle_root));
    Ok(DagUnit {
        unit_id,
        proposer_node,
        round: round_id,
        transactions: reconstructed_transactions,
        parent_units,
        merkle_root: batch_merkle_root, // ✅ Store actual root here
        finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    })
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


/// Verifies a Merkle proof for a given leaf, its branch, and the expected root.
///
/// # Arguments
/// * `leaf_hash` - The hash of the leaf node (i.e., tx.root)
/// * `proof` - A vector of sibling hashes from the leaf to the root
/// * `expected_root` - The root hash of the Merkle tree
/// * `index` - The index of the leaf in the original tree (used to determine left/right sibling order)
///
/// # Returns
/// `true` if the proof is valid, `false` otherwise
pub fn verify_merkle_proof(
    leaf_hash: &[u8],
    proof: &Vec<Vec<u8>>,
    expected_root: &[u8],
    mut index: usize,
) -> bool {
    let mut computed_hash = leaf_hash.to_vec();

    for sibling_hash in proof {
        let mut hasher = Sha256::new();

        if index % 2 == 0 {
            // Current node is on the left
            hasher.update(&computed_hash);
            hasher.update(sibling_hash);
        } else {
            // Current node is on the right
            hasher.update(sibling_hash);
            hasher.update(&computed_hash);
        }

        computed_hash = hasher.finalize().to_vec();
        index /= 2;
    }

    computed_hash == expected_root
}
