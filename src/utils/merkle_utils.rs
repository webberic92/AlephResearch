use sha2::{Digest, Sha256};
use tracing::{error, info};
use base64::{engine::general_purpose, Engine as _};

pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    if hashes.len() == 1 {
        info!("Final Merkle root computed: {:?}", hashes[0]);
        return hashes[0].clone();
    }

    let next_level: Vec<Vec<u8>> = hashes
        .chunks(2)
        .map(|pair| {
            let combined = if pair.len() == 2 {
                let mut combined = pair[0].clone();
                combined.extend(&pair[1]); // Left + Right
                combined
            } else {
                let mut combined = pair[0].clone();
                combined.extend(vec![0; 32]); // Add padding for odd pairs
                combined
            };
            let combined_hash = Sha256::digest(&combined).to_vec();
            info!(
                "Merkle Root Level {}: Pair {:?} + {:?} = Combined Hash: {:?}",
                hashes.len(),
                pair[0],
                pair.get(1).unwrap_or(&vec![0; 32]),
                combined_hash
            );
            combined_hash
        })
        .collect();

    compute_merkle_root(&next_level)
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

        info!(
            "Branch Level {}: Current Index = {}, Sibling Index = {}, Combined Hash = {:?}",
            current_level.len(),
            current_index,
            sibling_index,
            branch.last().unwrap()
        );

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

    info!("Computed Merkle branch for index {}: {:?}", index, branch);
    branch
}

/// Validate Merkle branch for a specific index and return the computed root
/// Validate Merkle branch for a specific index and return the computed root
pub fn validate_merkle_branch(
    shard_hashes: &[Vec<u8>],
    proofs: &[Vec<u8>],
    index: usize,
    expected_root: &[u8],
) -> bool {
    let mut current_hash = shard_hashes[index].clone();
    let mut current_index = index;

    // Log the initial state
    info!(
        "Starting Merkle branch validation. Initial hash: {:?}, Index: {}, Proofs: {:?}, Expected root: {:?}",
        current_hash, current_index, proofs, expected_root
    );

    for (i, sibling_hash) in proofs.iter().enumerate() {
        let mut combined = if current_index % 2 == 0 {
            current_hash.clone()
        } else {
            sibling_hash.clone()
        };
        combined.extend(if current_index % 2 == 0 {
            sibling_hash.clone()
        } else {
            current_hash.clone()
        });

        // Compute the next hash
        let combined_hash = Sha256::digest(&combined).to_vec();
        info!(
            "Step {}: Current index = {}, Sibling hash = {:?}, Combined = {:?}, Combined hash = {:?}",
            i, current_index, sibling_hash, combined, combined_hash
        );

        current_hash = combined_hash;
        current_index /= 2;
    }

    // Final validation against the expected root
    if current_hash == expected_root {
        info!(
            "Validation succeeded. Computed root matches expected root: {:?}",
            expected_root
        );
        true
    } else {
        error!(
            "Validation failed. Computed root: {:?}, Expected root: {:?}",
            current_hash, expected_root
        );
        false
    }
}


/// Reconstruct the original unit from shards and validate using proofs
pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    proofs: &[Vec<Vec<u8>>],
    root: &[u8], // Expected Merkle root
) -> Result<Vec<u8>, String> {
    info!("Reconstructing unit from shards and verifying Merkle proof");

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

    info!("Transaction data size: {}", transaction_data.len());
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

    info!(
        "Shard size validation successful. Total shard size: {} matches transaction size: {}",
        total_size, transaction_size
    );
    Ok(())
}

