use sha2::{Digest, Sha256};
use tracing::{error, info};
use base64::{engine::general_purpose, Engine as _};

/// Compute Merkle root from shard hashes
pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    if hashes.len() == 1 {
        return hashes[0].clone();
    }
    let mut next_level = vec![];
    for pair in hashes.chunks(2) {
        let mut combined = pair[0].clone();
        if pair.len() > 1 {
            combined.extend(&pair[1]);
        }
        next_level.push(Sha256::digest(&combined).to_vec());
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

        // Add the sibling hash even if it doesn't exist (use a placeholder)
        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
        } else {
            // Placeholder for missing sibling
            branch.push(vec![]);
        }

        current_index /= 2;
        current_level = current_level
            .chunks(2)
            .map(|pair| {
                let mut combined = pair[0].clone();
                if pair.len() > 1 {
                    combined.extend(&pair[1]);
                }
                Sha256::digest(&combined).to_vec()
            })
            .collect();
    }

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
                proof_hash // Use the provided proof hash
            } else {
                error!(
                    "Missing proof hash at Level {}, Chunk {}. Proof: {:?}",
                    level, i, proof
                );
                return vec![];
            };

            let mut hasher = Sha256::new();
            hasher.update(left);
            hasher.update(right);
            next_level_hashes.push(hasher.finalize().to_vec());
        }

        current_hashes = next_level_hashes;
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
            "Reconstruction failed: computed root {:?} does not match provided root {:?}",
            computed_root, root
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
    
        #[test]
        fn test_merkle_branch_consistency() {
            // Example shard hashes
            let shard_hashes = vec![
                Sha256::digest(b"shard1").to_vec(),
                Sha256::digest(b"shard2").to_vec(),
                Sha256::digest(b"shard3").to_vec(),
                Sha256::digest(b"shard4").to_vec(),
            ];
    
            // Compute the Merkle branch and root
            let branch = compute_merkle_branch(&shard_hashes, 0);
            let computed_root = validate_merkle_branch(&shard_hashes, &[branch.clone()]);
    
            // Recalculate the root using just the branch
            assert_eq!(
                computed_root,
                compute_merkle_branch(&shard_hashes, 0)[0]
            );
        }
    
        #[test]
        fn test_invalid_merkle_proof() {
            let shard_hashes = vec![
                Sha256::digest(b"shard1").to_vec(),
                Sha256::digest(b"shard2").to_vec(),
            ];
            let invalid_proofs = vec![vec![b"invalid_proof".to_vec()]];
    
            let computed_root = validate_merkle_branch(&shard_hashes, &invalid_proofs);
    
            assert_eq!(computed_root, Vec::<u8>::new(), "Invalid proof should fail");
        }
    }