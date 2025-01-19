use sha2::{Digest, Sha256};
use tracing::{error, info};

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
pub fn validate_merkle_branch(shards: &[Vec<u8>], proofs: &[Vec<Vec<u8>>]) -> Vec<u8> {
    info!("Validating Merkle branch");

    if shards.len() != proofs.len() {
        error!(
            "Mismatch in lengths: shards ({}) vs proofs ({})",
            shards.len(),
            proofs.len()
        );
        return Vec::new();
    }

    // Start with the hash of each shard
    let mut validated_hashes = Vec::new();

    for (shard, shard_proofs) in shards.iter().zip(proofs.iter()) {
        // Start with the hash of the shard
        let mut hash = Sha256::digest(shard).to_vec();

        // Combine with sibling hashes from the proof
        for sibling in shard_proofs {
            let combined = if hash < *sibling {
                [hash.clone(), sibling.clone()].concat()
            } else {
                [sibling.clone(), hash.clone()].concat()
            };
            hash = Sha256::digest(&combined).to_vec();
        }

        validated_hashes.push(hash);
    }

    let root = compute_merkle_root(&validated_hashes);
    info!("Merkle branches validated. Computed root: {:?}", root);

    root
}


/// Reconstruct the original unit from shards and validate using proofs
pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    proofs: &[Vec<Vec<u8>>],
    root: &Vec<u8>, // Pass the expected Merkle root as a parameter
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

    // Combine all shards into a single reconstructed unit
    let mut reconstructed_unit = vec![];
    for shard in shards {
        reconstructed_unit.extend(shard);
    }

    // Compute the hashes of the shards
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();

    // Validate the Merkle branch and compare the computed root with the provided root
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

