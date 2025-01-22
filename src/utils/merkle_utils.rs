use sha2::{Digest, Sha256};
use tracing::{error, info};
use base64::{engine::general_purpose, Engine as _};

/// Compute Merkle root from shard hashes
/// Compute Merkle root from shard hashes
pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    if hashes.len() == 1 {
        info!("Computed Merkle root: {:?}", hashes[0]);
        return hashes[0].clone();
    }

    let mut next_level = vec![];
    for pair in hashes.chunks(2) {
        let mut combined = pair[0].clone();
        if pair.len() > 1 {
            combined.extend(&pair[1]);
        }
        let combined_hash = Sha256::digest(&combined).to_vec();
        
        // Clone `combined_hash` for logging to avoid move issues
        info!(
            "Pair: {:?} + {:?} = Combined hash: {:?}",
            pair[0],
            pair.get(1).unwrap_or(&vec![0; 32]), // Placeholder for missing sibling
            combined_hash.clone()
        );
        
        // Push the combined hash into the next level
        next_level.push(combined_hash);
    }

    compute_merkle_root(&next_level)
}



/// Compute the Merkle branch for a given index in the Merkle tree
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
            branch.push(vec![0; 32]); // Zero-filled placeholder hash
        }

        info!(
            "Index {}: Adding sibling {:?} to branch",
            sibling_index, branch.last().unwrap()
        );

        current_index /= 2;
        current_level = current_level
            .chunks(2)
            .map(|pair| {
                let mut combined = pair[0].clone();
                if pair.len() > 1 {
                    combined.extend(&pair[1]);
                }
                let digest = Sha256::digest(&combined).to_vec();
                info!("Intermediate combined hash: {:?}", digest);
                digest
            })
            .collect();
    }

    info!("Computed Merkle branch for index {}: {:?}", index, branch);
    branch
}




/// Validate Merkle branches and return the root
pub fn validate_merkle_branch(
    shard_hashes: &[Vec<u8>],
    proofs: &[Vec<Vec<u8>>],
) -> Vec<u8> {
    info!(
        "Validating Merkle branch with {} shard hashes and {} levels of proofs",
        shard_hashes.len(),
        proofs.len()
    );

    let mut current_hashes = shard_hashes.to_vec();

    for (level, proof) in proofs.iter().enumerate() {
        let mut next_level_hashes = vec![];

        for (i, chunk) in current_hashes.chunks(2).enumerate() {
            let left = &chunk[0];
            let right = if chunk.len() > 1 {
                &chunk[1]
            } else if let Some(proof_hash) = proof.get(i) {
                // Use the proof hash for missing sibling
                proof_hash
            } else {
                // Log an error for missing proof hash
                error!(
                    "Missing proof hash at Level {}, Chunk {}. Proof: {:?}",
                    level, i, proof
                );
                return vec![];
            };

            // Log intermediate values for debugging
            info!(
                "Level {}, Chunk {}: Left = {:?}, Right = {:?}",
                level, i, left, right
            );

            // Compute the hash of the combined chunk
            let mut hasher = Sha256::new();
            hasher.update(left);
            hasher.update(right);
            let combined_hash = hasher.finalize().to_vec();

            // Clone combined_hash for logging and pushing into next_level_hashes
            info!(
                "Level {}, Chunk {}: Computed combined hash = {:?}",
                level, i, combined_hash.clone() // Clone here for logging
            );
            next_level_hashes.push(combined_hash); // Move into vector
        }

        // Update current hashes to the next level
        current_hashes = next_level_hashes;

        // Log the intermediate Merkle tree level
        info!(
            "After Level {}: Intermediate Merkle hashes = {:?}",
            level, current_hashes
        );
    }

    if current_hashes.len() == 1 {
        info!("Successfully computed Merkle root: {:?}", current_hashes[0]);
        current_hashes[0].clone()
    } else {
        error!(
            "Failed to compute a single root hash. Remaining hashes: {:?}",
            current_hashes
        );
        vec![]
    }
}





/// Reconstruct the original unit from shards and validate using proofs
pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    proofs: &[Vec<Vec<u8>>],
    root: &Vec<u8>, // Expected Merkle root
) -> Result<Vec<u8>, String> {
    info!("Reconstructing unit from shards and verifying Merkle proof");
    
    info!("Raw serialized shards: {:?}", shards);
    info!("Raw serialized proofs: {:?}", proofs);
    
    let serialized_shards: Vec<String> = shards
        .iter()
        .map(|shard| general_purpose::STANDARD.encode(shard))
        .collect();
    let serialized_proofs: Vec<Vec<String>> = proofs
    .iter()
        .map(|proof| proof.iter().map(|p| general_purpose::STANDARD.encode(p)).collect())
        .collect();

    info!("Serialized shards for transmission: {:?}", serialized_shards);
    info!("Serialized proofs for transmission: {:?}", serialized_proofs);

    // Validate inputs
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

    // Combine shards into a single unit
    let mut reconstructed_unit = vec![];
    for shard in shards {
        reconstructed_unit.extend(shard);
    }

    // Compute the hashes of the shards
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    info!("Shard hashes for reconstruction: {:?}", shard_hashes);

    // Validate Merkle proof and compare computed root with provided root
    let computed_root = validate_merkle_branch(&shard_hashes, proofs);
    if computed_root != *root {
        let error_message = format!(
            "Reconstruction failed: computed root {:?} does not match provided root {:?}. Shards: {:?}, Proofs: {:?}",
            computed_root, root, shards, proofs
        );
        error!("{}", error_message);
        return Err(error_message);
    }
    
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
    info!(
        "Shard sizes: {:?}",
        shards.iter().map(|s| s.len()).collect::<Vec<_>>()
    );
    info!(
        "Total size of all shards: {}",
        shards.iter().map(|s| s.len()).sum::<usize>()
    );
    assert_eq!(
        shards.len(),
        data_shards,
        "Shard count mismatch: expected {}, found {}",
        data_shards,
        shards.len()
    );
    
    for (i, shard) in shards.iter().enumerate() {
        info!(
            "Shard {}: Size = {}, Data = {:?}",
            i,
            shard.len(),
            &shard[0..std::cmp::min(10, shard.len())] // Log only the first 10 bytes for readability
        );
    }

    shards
}

pub fn validate_shard_sizes(shards: &[Vec<u8>], transaction_size: usize) -> Result<(), String> {
    // Calculate the total size of all shards
    let total_size: usize = shards.iter().map(|shard| shard.len()).sum();

    // Check if the total size matches the transaction size
    if total_size != transaction_size {
        let error_message = format!(
            "Shard size validation failed. Total shard size: {}, Expected transaction size: {}",
            total_size, transaction_size
        );
        error!("{}", error_message);
        return Err(error_message); // Return an error if validation fails
    }

    info!(
        "Shard size validation successful. Total shard size: {} matches transaction size: {}",
        total_size, transaction_size
    );
    Ok(()) // Return Ok if validation passes
}



#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Sha256;

    #[test]
    fn test_compute_merkle_root() {
        let data = vec![
            b"shard1".to_vec(),
            b"shard2".to_vec(),
            b"shard3".to_vec(),
            b"shard4".to_vec(),
        ];
        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
        let root = compute_merkle_root(&hashes);

        assert!(
            !root.is_empty(),
            "Computed Merkle root is empty. Data: {:?}, Hashes: {:?}",
            data,
            hashes
        );
        info!("Test: Computed Merkle root = {:?}", root);
    }

    #[test]
    fn test_compute_merkle_branch() {
        let data = vec![
            b"shard1".to_vec(),
            b"shard2".to_vec(),
            b"shard3".to_vec(),
            b"shard4".to_vec(),
        ];
        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
        let branch = compute_merkle_branch(&hashes, 0);

        assert!(
            !branch.is_empty(),
            "Computed Merkle branch is empty. Data: {:?}, Hashes: {:?}",
            data,
            hashes
        );
        info!("Test: Computed Merkle branch for index 0 = {:?}", branch);
    }

    #[test]
    fn test_validate_merkle_branch() {
        let data = vec![
            b"shard1".to_vec(),
            b"shard2".to_vec(),
            b"shard3".to_vec(),
            b"shard4".to_vec(),
        ];
        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
        let proofs: Vec<Vec<Vec<u8>>> = hashes
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(&hashes, i))
            .collect();
        let root = compute_merkle_root(&hashes);

        let computed_root = validate_merkle_branch(&hashes, &proofs);
        assert_eq!(
            computed_root, root,
            "Validation failed. Computed root: {:?}, Expected root: {:?}",
            computed_root, root
        );
        info!("Test: Validation succeeded. Computed root = {:?}", computed_root);
    }

    #[test]
    fn test_split_into_shards() {
        let data = vec![1; 256];
        let shards = split_into_shards(&data, 4);

        assert_eq!(
            shards.len(),
            4,
            "Shard count mismatch. Expected: 4, Got: {}",
            shards.len()
        );
        assert!(
            shards.iter().all(|s| s.len() == 64),
            "Shard size mismatch. Shards: {:?}",
            shards
        );
        info!("Test: Split into shards succeeded. Shards = {:?}", shards);
    }

    #[test]
    fn test_validate_shard_sizes() {
        let data = vec![1; 256];
        let shards = split_into_shards(&data, 4);

        assert!(
            validate_shard_sizes(&shards, 256).is_ok(),
            "Shard size validation failed. Shards: {:?}, Total size: {}",
            shards,
            data.len()
        );
        info!("Test: Shard size validation succeeded.");
    }

    #[test]
    fn test_reconstruct_unit() {
        let data = vec![1; 256];
        let shards = split_into_shards(&data, 4);
        let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
        let proofs = shard_hashes
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(&shard_hashes, i))
            .collect::<Vec<_>>();
        let root = compute_merkle_root(&shard_hashes);
    
        let reconstructed = reconstruct_unit(&shards, &proofs, &root);
    
        assert!(
            reconstructed.is_ok(),
            "Reconstruction failed: {:?}",
            reconstructed.as_ref().err() // Use `as_ref` to borrow the error without moving
        );
    
        assert_eq!(
            reconstructed.as_ref().unwrap(), // Use `as_ref` to borrow the success value
            &data,
            "Reconstructed data mismatch. Original: {:?}, Reconstructed: {:?}",
            data,
            reconstructed.as_ref().unwrap() // Use `as_ref` again for consistent borrowing
        );
    
        info!("Test: Reconstruction succeeded.");
    }
    

    #[test]
    fn test_payload_serialization() {
        let shards = vec![
            b"shard1".to_vec(),
            b"shard2".to_vec(),
            b"shard3".to_vec(),
            b"shard4".to_vec(),
        ];
        let serialized_shards: Vec<String> = shards
            .iter()
            .map(|shard| general_purpose::STANDARD.encode(shard))
            .collect();
        let deserialized_shards: Vec<Vec<u8>> = serialized_shards
            .iter()
            .map(|shard| general_purpose::STANDARD.decode(shard).expect("Failed to decode shard"))
            .collect();

        assert_eq!(shards, deserialized_shards);
        info!("Test: Payload serialization succeeded.");
    }
}
