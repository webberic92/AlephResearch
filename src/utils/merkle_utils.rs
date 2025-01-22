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

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber;

    fn init_logger() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .try_init();
    }

    #[test]
    fn test_single_node_merkle_tree() {
        init_logger();

        let hashes = vec![vec![1; 32]];
        let root = compute_merkle_root(&hashes);
        assert_eq!(root, hashes[0], "Single node Merkle root mismatch");
    }

    #[test]
    fn test_odd_length_merkle_tree() {
        init_logger();

        let data = vec![vec![1; 32], vec![2; 32], vec![3; 32]];
        let root = compute_merkle_root(&data);
        assert!(!root.is_empty(), "Root should not be empty for odd-length tree");
    }

    #[test]
    fn test_validate_merkle_branch_minimal() {
        init_logger();
    
        // Generate 256-byte transaction data
        let data: Vec<Vec<u8>> = (0..4)
            .map(|i| {
                let mut shard = vec![0; 256];
                shard[0..6].copy_from_slice(format!("shard{}", i + 1).as_bytes());
                shard
            })
            .collect();
    
        // Compute hashes for each shard
        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
    
        // Compute the Merkle root
        let root = compute_merkle_root(&hashes);
    
        // Generate proofs for each shard
        let proofs: Vec<Vec<Vec<u8>>> = (0..hashes.len())
            .map(|i| compute_merkle_branch(&hashes, i))
            .collect();
    
        // Validate each shard's proof
        for (i, proof) in proofs.iter().enumerate() {
            assert!(
                validate_merkle_branch(&hashes, proof, i, &root),
                "Validation failed for shard index {}",
                i
            );
        }
    }
    #[test]
fn test_reconstruct_unit() {
    init_logger();

    // Step 1: Generate 256-byte transaction data
    let transaction_data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect(); // 1024 bytes of data
    let shard_count = 4;

    // Step 2: Split data into shards
    let shards = split_into_shards(&transaction_data, shard_count);

    // Step 3: Compute hashes for each shard
    let hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();

    // Step 4: Compute the Merkle root
    let root = compute_merkle_root(&hashes);

    // Step 5: Generate Merkle proofs for each shard
    let proofs: Vec<Vec<Vec<u8>>> = (0..hashes.len())
        .map(|i| compute_merkle_branch(&hashes, i))
        .collect();

    // Step 6: Attempt to reconstruct the unit and validate
    match reconstruct_unit(&shards, &proofs, &root) {
        Ok(reconstructed_data) => {
            assert_eq!(
                reconstructed_data, transaction_data,
                "Reconstructed data does not match the original transaction data"
            );
            info!("Reconstruction and validation succeeded!");
        }
        Err(error_message) => {
            panic!("Reconstruction failed: {}", error_message);
        }
    }
}

#[test]
fn test_propose_integration() {
    init_logger();
    info!("Starting test_propose_integration...");

    // Step 1: Generate transaction data
    let transaction_data = (0..256).map(|i| i as u8).collect::<Vec<_>>();
    info!("Generated transaction data (size: {} bytes): {:?}", transaction_data.len(), transaction_data);

    // Step 2: Split transaction data into shards
    let shard_count = 4;
    let shards = split_into_shards(&transaction_data, shard_count);
    info!("Split transaction data into {} shards:", shard_count);
    for (i, shard) in shards.iter().enumerate() {
        info!("Shard {} (size: {} bytes): {:?}", i, shard.len(), shard);
    }
    info!("Transaction data: {:?}", transaction_data);

    
    // Step 3: Compute hashes for each shard
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();
    info!("Computed shard hashes:");
    for (i, hash) in shard_hashes.iter().enumerate() {
        info!("Shard {} Hash: {:?}", i, hash);
    }
    info!("Computed shard hashes: {:?}", shard_hashes);
    // Step 4: Compute the Merkle root
    let root = compute_merkle_root(&shard_hashes);
    info!("Computed Merkle root: {:?}", root);

    // Step 5: Generate Merkle proofs for each shard
    let proofs: Vec<Vec<Vec<u8>>> = (0..shard_hashes.len())
        .map(|i| compute_merkle_branch(&shard_hashes, i))
        .collect();
    info!("Generated Merkle proofs for each shard:");
    for (i, proof) in proofs.iter().enumerate() {
        info!("Proof for Shard {}: {:?}", i, proof);
    }
    info!("Generated Merkle proofs: {:?}", proofs);

    // Step 6: Encode shards and proofs
    let encoded_shards: Vec<String> = shards
        .iter()
        .map(|shard| general_purpose::STANDARD.encode(shard))
        .collect();
    let encoded_proofs: Vec<Vec<String>> = proofs
        .iter()
        .map(|proof| {
            proof
                .iter()
                .map(|p| general_purpose::STANDARD.encode(p))
                .collect::<Vec<_>>()
        })
        .collect();
    info!("Encoded shards: {:?}", encoded_shards);
    info!("Encoded proofs: {:?}", encoded_proofs);

    // Step 7: Decode shards and proofs
    let decoded_shards: Vec<Vec<u8>> = encoded_shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard).expect("Shard decoding failed"))
        .collect();
    let decoded_proofs: Vec<Vec<Vec<u8>>> = encoded_proofs
        .iter()
        .map(|proof| {
            proof
                .iter()
                .map(|p| general_purpose::STANDARD.decode(p).expect("Proof decoding failed"))
                .collect::<Vec<_>>()
        })
        .collect();
    info!("Successfully decoded shards and proofs:");
    info!("Decoded shards: {:?}", decoded_shards);
    info!("Decoded proofs: {:?}", decoded_proofs);

    // Step 8: Validate Merkle branches
    for (i, proof) in decoded_proofs.iter().enumerate() {
        info!(
            "Validating Merkle branch for Shard {}: Proof = {:?}, Root = {:?}",
            i, proof, root
        );
        assert!(
            validate_merkle_branch(&decoded_shards, proof, i, &root),
            "Validation failed for Shard {}. Proof = {:?}, Expected Root = {:?}",
            i, proof, root
        );
    }
    info!("All Merkle branches validated successfully!");

    // Step 9: Test reconstruction
    info!("Starting reconstruction of transaction data...");
    match reconstruct_unit(&decoded_shards, &decoded_proofs, &root) {
        Ok(reconstructed_data) => {
            assert_eq!(
                reconstructed_data, transaction_data,
                "Reconstructed data does not match the original transaction data"
            );
            info!("Reconstruction and validation succeeded! Reconstructed data matches the original.");
        }
        Err(error_message) => {
            error!("Reconstruction failed: {}", error_message);
            panic!("Reconstruction failed: {}", error_message);
        }
    }
}

    
}
