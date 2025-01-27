use sha2::{Digest, Sha256};
use tracing::{debug, error, info};

use crate::structs::dag::ReconstructedUnit;


pub fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    let mut current_level = hashes.to_vec();
    info!("Initial level for Merkle root computation: {:?}", current_level);

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

        info!("Next level of Merkle tree: {:?}", current_level);
    }

    let root = current_level[0].clone();
    info!("Computed Merkle root: {:?}", root);
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

        debug!(
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

    debug!("Computed Merkle branch for index {}: {:?}", index, branch);
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

    for (level, sibling_hash) in proofs.iter().enumerate() {
        let combined = if current_index % 2 == 0 {
            [current_hash.clone(), sibling_hash.clone()].concat()
        } else {
            [sibling_hash.clone(), current_hash.clone()].concat()
        };

        debug!(
            "Validation Level {}: Current Hash = {:?}, Sibling Hash = {:?}, Combined Hash = {:?}",
            level, current_hash, sibling_hash, combined
        );

        current_hash = Sha256::digest(&combined).to_vec();
        current_index /= 2;
    }

    debug!(
        "Validation result: Final hash = {:?}, Expected root = {:?}",
        current_hash, expected_root
    );

    current_hash == expected_root
}



pub fn reconstruct_unit(
    shards: &[Vec<u8>],
    epoch_id: u64,
    parent_hashes: Vec<u8>, // Flat parent hashes in binary format
) -> Result<ReconstructedUnit, String> {
    if shards.is_empty() {
        return Err("Reconstruction failed: shards are empty".to_string());
    }

    // Compute the hashes of the individual shards
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|shard| Sha256::digest(shard).to_vec()).collect();

    // Compute the Merkle root using shard hashes
    let root = compute_merkle_root(&shard_hashes);

    info!(
        "Reconstructing unit: Concatenated data = {:?}, Computed root = {:?}",
        shards.concat(),
        root
    );

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

    info!("Reconstructed parents: {:?}", parents);

    Ok(ReconstructedUnit {
        data: shards.concat(),
        root,
        parents,
        epoch_id,
    })
}


pub fn split_into_shards(transaction_data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
    let shard_size = transaction_data.len() / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    info!("Shards split into {} parts: {:?}", data_shards, shards);
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

