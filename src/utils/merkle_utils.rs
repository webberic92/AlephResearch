use base64::Engine;
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};
use tracing::{ error, info};

use crate::structs::dag::ReconstructedUnit;


pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    let mut current_level = hashes.to_vec();
    // info!("Initial level for Merkle root computation: {:?}", current_level);

    while current_level.len() > 1 {
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

        // info!("Next level of Merkle tree: {:?}", current_level);
    }

    let root = current_level[0].clone();
    // info!("Computed Merkle root: {:?}", root);
    root
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
    shard_hashes: &[Vec<u8>],
    proofs: &[Vec<u8>],
    index: usize,
    expected_root: &[u8],
) -> bool {
    let mut current_hash = shard_hashes[index].clone();
    let mut current_index = index;

    for (_level, sibling_hash) in proofs.iter().enumerate() {
        let combined = if current_index % 2 == 0 {
            [current_hash.clone(), sibling_hash.clone()].concat()
        } else {
            [sibling_hash.clone(), current_hash.clone()].concat()
        };

        // debug!(
        //     "Validation Level {}: Current Hash = {:?}, Sibling Hash = {:?}, Combined Hash = {:?}",
        //     level, current_hash, sibling_hash, combined
        // );

        current_hash = Sha256::digest(&combined).to_vec();
        current_index /= 2;
    }

    // debug!(
    //     "Validation result: Final hash = {:?}, Expected root = {:?}",
    //     current_hash, expected_root
    // );

    current_hash == expected_root
}



pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    round_id: u64,
    parent_hashes: Vec<String>, // ✅ Unit IDs as strings, NOT Base64-encoded hashes
) -> Result<ReconstructedUnit, String> {
    if shards.is_empty() {
        return Err("Reconstruction failed: shards are empty".to_string());
    }

    // Compute the hashes of the individual shards
    let shard_hashes: Vec<Vec<u8>> = shards.iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();
    info!("shard_hashes : {:?}", shard_hashes);

    // Compute the Merkle root using shard hashes
    let root = compute_merkle_root(&shard_hashes);
    info!("root : {:?}", root);

    info!(
        "Reconstructing unit: Concatenated data = {:?}, Computed root = {:?}",
        shards.concat(),
        root
    );

    // ✅ **Fix: Convert parent unit IDs into raw bytes**
    let parents: Vec<Vec<u8>> = parent_hashes
        .iter()
        .map(|parent| parent.as_bytes().to_vec()) // ✅ Convert directly to bytes (No Base64 decoding)
        .collect();

    info!("Reconstructed parents: {:?}", parents);
    info!("Reconstructed parents successfully");

    Ok(ReconstructedUnit {
        data: shards.concat(),
        root,
        parents,
        round_id,
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


pub fn interpolate_shares(shards: &[Vec<u8>], round_id: u64) -> Result<Vec<Vec<u8>>, String> {
    if shards.is_empty() {
        return Err("Interpolation failed: No available shards".to_string());
    }

    // ✅ Handle first transaction (epoch 1): No interpolation needed
    if round_id == 1 {
        info!("Epoch 1 detected: Skipping interpolation, returning provided shards.");
        return Ok(shards.to_vec()); // ✅ Just return original shards
    }

    let data_shards = shards.len() / 2; // Assumption: f+1 out of N shards
    let parity_shards = shards.len() - data_shards;

    if data_shards < 1 {
        return Err(format!(
            "Interpolation failed: Not enough data shards ({}). Requires at least 1.",
            data_shards
        ));
    }

    // Initialize Reed-Solomon erasure coding
    let r = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| format!("Failed to create Reed-Solomon codec: {:?}", e))?;

    // Clone and pad the shards (Reed-Solomon needs full set)
    let mut shard_buffer: Vec<Option<Vec<u8>>> = shards.iter().map(|s| Some(s.clone())).collect();
    shard_buffer.resize(data_shards + parity_shards, None);

    // Recover missing shares
    r.reconstruct(&mut shard_buffer)
        .map_err(|e| format!("Failed to interpolate shares: {:?}", e))?;

    // Convert back to Vec<Vec<u8>>
    let recovered_shards: Vec<Vec<u8>> = shard_buffer
        .into_iter()
        .filter_map(|s| s) // Remove None values
        .collect();

    info!("Interpolated missing shares successfully.");
    Ok(recovered_shards)
}
