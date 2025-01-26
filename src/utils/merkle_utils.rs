use sha2::{Digest, Sha256};
use tracing::{error, info};
use base64::{engine::general_purpose, Engine as _};


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

/// Validate Merkle branch for a specific index and return the computed root
/// Validate Merkle branch for a specific index and return the computed root
// pub fn validate_merkle_branch(
//     shard_hashes: &[Vec<u8>],
//     proofs: &[Vec<u8>],
//     index: usize,
//     expected_root: &[u8],
// ) -> bool {
//     if index >= shard_hashes.len() {
//         error!(
//             "Invalid index: {} (shard_hashes length: {})",
//             index, shard_hashes.len()
//         );
//         return false;
//     }

//     let mut current_hash = shard_hashes[index].clone();
//     let mut current_index = index;

//     for sibling_hash in proofs {
//         let sibling_index = if current_index == 0 {
//             0 // No valid sibling; default behavior
//         } else {
//             current_index - 1
//         };

//         let mut combined = if current_index % 2 == 0 {
//             current_hash.clone()
//         } else {
//             sibling_hash.clone()
//         };

//         combined.extend(if current_index % 2 == 0 {
//             sibling_hash.clone()
//         } else {
//             current_hash.clone()
//         });

//         current_hash = Sha256::digest(&combined).to_vec();
//         current_index /= 2;
//     }

//     current_hash == expected_root
// }

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

    for sibling_hash in proofs {
        let mut combined = if current_index % 2 == 0 {
            [current_hash.clone(), sibling_hash.clone()].concat()
        } else {
            [sibling_hash.clone(), current_hash.clone()].concat()
        };

        current_hash = Sha256::digest(&combined).to_vec();
        current_index /= 2;
    }

    current_hash == expected_root
}


/// Reconstruct the original unit from shards and validate using proofs
pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    proofs: &[Vec<Vec<u8>>],
    root: &[u8], // Expected Merkle root
) -> Result<Vec<u8>, String> {
    // info!("Reconstructing unit from shards and verifying Merkle proof");

    if shards.is_empty() || proofs.is_empty() {
        let error_message = "Reconstruction failed: shards or proofs are empty".to_string();
        error!("{}", error_message);
        return Err(error_message);
    }

    if shards.len() != proofs.len() {
        let error_message = format!(
            "Reconstruction failed: number of shards ({}) does not match number of proofs ({})",
            shards.len(),
            proofs.len()
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();

    for (i, proof) in proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, i, root) {
            return Err(format!(
                "Validation failed for shard index {}. Proof: {:?}, Root: {:?}",
                i, proof, root
            ));
        }
    }

    let reconstructed_unit = shards.concat();
    info!("Successfully reconstructed unit and verified Merkle root");
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

