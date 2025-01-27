use sha2::{Digest, Sha256};
use tracing::{error, info};

use crate::structs::dag::ReconstructedUnit;

pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    let mut current_level = hashes.to_vec();

    while current_level.len() > 1 {
        let mut next_level = Vec::new();

        for pair in current_level.chunks(2) {
            let hash = if pair.len() == 2 {
                let mut hasher = Sha256::new();
                hasher.update(&pair[0]);
                hasher.update(&pair[1]);
                hasher.finalize().to_vec()
            } else {
                pair[0].clone() // For odd number of nodes, promote the last node
            };

            next_level.push(hash);
        }

        current_level = next_level;
    }

    current_level.first().cloned().unwrap_or_else(|| vec![]) // Return root or empty Vec
}


// Updated `compute_merkle_branch` with correct combination order
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

        // info!(
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

    // info!("Computed Merkle branch for index {}: {:?}", index, branch);
    branch
}


pub fn validate_merkle_branch(
    shard_hashes: &[Vec<u8>],
    proofs: &[Vec<u8>],
    index: usize,
    expected_root: &[u8],
) -> bool {
    if index >= shard_hashes.len() {
        error!(
            "Invalid index: {} (shard_hashes length: {})",
            index, shard_hashes.len()
        );
        return false;
    }

    let mut current_hash = shard_hashes[index].clone();
    let mut current_index = index;

    for (level, sibling_hash) in proofs.iter().enumerate() {
        let combined = if current_index % 2 == 0 {
            [current_hash.clone(), sibling_hash.clone()].concat()
        } else {
            [sibling_hash.clone(), current_hash.clone()].concat()
        };

        // info!(
        //     "Node {}: Validation Level {} - Current Hash: {:?}, Sibling Hash: {:?}, Combined Hash: {:?}",
        //     level, current_index, current_hash, sibling_hash, combined
        // );

        current_hash = Sha256::digest(&combined).to_vec();
        current_index /= 2;
    }

    if current_hash == expected_root {
        // info!(
        //     "Validation succeeded: Final root matches expected root. Computed: {:?}, Expected: {:?}",
        //     current_hash, expected_root
        // );
    } else {
        error!(
            "Validation failed: Final root does not match expected root. Computed: {:?}, Expected: {:?}",
            current_hash, expected_root
        );
    }

    current_hash == expected_root
}



/// Reconstruct the original unit from shards and validate using proofs
pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    epoch_id: u64,
    parent_hashes: Vec<u8>, // Flat parent hashes in binary format
) -> Result<ReconstructedUnit, String> {
    if shards.is_empty() {
        return Err("Reconstruction failed: shards are empty".to_string());
    }

    // Combine shards to reconstruct the unit data
    let data = shards.concat();

    // Compute the Merkle root for the reconstructed data
    let root = Sha256::digest(&data).to_vec();

    // Split flat parent hashes into individual hashes (32 bytes each)
    const HASH_SIZE: usize = 32;
    if parent_hashes.len() % HASH_SIZE != 0 {
        return Err(format!(
            "Invalid parent hashes size: expected multiple of {}, got {}",
            HASH_SIZE, parent_hashes.len()
        ));
    }

    let parents: Vec<Vec<u8>> = parent_hashes
        .chunks(HASH_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();

    // Construct the ReconstructedUnit object
    let reconstructed_unit = ReconstructedUnit {
        data,
        root,
        parents,
        epoch_id,
    };

    Ok(reconstructed_unit)
}


/// Splits transaction data into shards
pub fn split_into_shards(transaction_data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
    let shard_size = transaction_data.len() / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    // info!("Transaction data size: {}", transaction_data.len());
    assert_eq!(
        shards.len(),
        data_shards,
        "Shard count mismatch: expected {}, found {}",
        data_shards,
        shards.len()
    );

    shards
}/// Splits transaction data into shards
// pub fn split_into_shards(transaction_data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
//     let shard_size = transaction_data.len() / data_shards;
//     let shards: Vec<Vec<u8>> = transaction_data
//         .chunks(shard_size)
//         .map(|chunk| chunk.to_vec())
//         .collect();

//     // info!("Transaction data size: {}", transaction_data.len());
//     assert_eq!(
//         shards.len(),
//         data_shards,
//         "Shard count mismatch: expected {}, found {}",
//         data_shards,
//         shards.len()
//     );

//     shards
// }

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

