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

/// Validate a Merkle branch and return the computed root
pub fn validate_merkle_branch(shard: &[u8], proof: &[Vec<u8>]) -> Vec<u8> {
    info!("Validating merkle branch");

    let mut hash = Sha256::digest(shard).to_vec();
    for sibling in proof {
        let combined = if hash < *sibling {
            [hash.clone(), sibling.clone()].concat()
        } else {
            [sibling.clone(), hash.clone()].concat()
        };
        hash = Sha256::digest(&combined).to_vec();
    }
    info!("Done validating merkle branch");
    hash
}

/// Reconstruct the original unit from shards and proof
pub fn reconstruct_unit(shards: &[Vec<u8>], proof: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    info!("Reconstructing unit from shards");

    // Validate the number of shards
    if shards.is_empty() || proof.is_empty() {
        error!("Shards or proof is empty during reconstruction");
        return Err("Shards or proof is empty".to_string());
    }

    // Combine all shards into a single unit
    let mut reconstructed_unit = vec![];
    for shard in shards {
        reconstructed_unit.extend(shard);
    }

    // Compute the Merkle root for verification
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let computed_root = compute_merkle_root(&shard_hashes);

    // Verify the computed root against the provided proof
    if computed_root != proof[0] {
        error!(
            "Reconstruction failed: computed root {:?} does not match proof root {:?}",
            computed_root, proof[0]
        );
        return Err("Reconstructed Merkle root does not match proof".to_string());
    }

    info!("Successfully reconstructed unit and verified Merkle root");
    Ok(reconstructed_unit)
}