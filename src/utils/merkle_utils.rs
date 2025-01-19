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

        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
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
    let mut current_hashes = shard_hashes.to_vec();

    // Log initial shard hashes
    info!("Initial shard hashes: {:?}", current_hashes);

    for (level, proof) in proofs.iter().enumerate() {
        let mut next_level_hashes = vec![];

        for (i, chunk) in current_hashes.chunks(2).enumerate() {
            let left = &chunk[0];
            let right = if chunk.len() > 1 {
                &chunk[1]
            } else {
                // Single hash on this level (no sibling)
                left
            };

            // Compute the combined hash
            let mut hasher = Sha256::new();
            hasher.update(left);
            hasher.update(right);
            let combined_hash = hasher.finalize().to_vec();

            // Log each hash computation
            info!(
                "Level {}: Combining chunk {} -> Left: {:?}, Right: {:?}, Combined: {:?}",
                level, i, left, right, combined_hash
            );

            next_level_hashes.push(combined_hash);
        }

        // Append proof hashes to the next level if provided
        for proof_hash in proof {
            info!("Level {}: Adding proof hash: {:?}", level, proof_hash);
            next_level_hashes.push(proof_hash.clone());
        }

        current_hashes = next_level_hashes;
    }

    // The last remaining hash should be the computed root
    if current_hashes.len() == 1 {
        info!("Computed Merkle root: {:?}", current_hashes[0]);
        current_hashes[0].clone()
    } else {
        error!(
            "Failed to compute a single root hash. Remaining hashes: {:?}",
            current_hashes
        );
        vec![] // Return an empty vector to indicate failure
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


