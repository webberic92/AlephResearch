use std::sync::Arc;
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use crate::{structs::node::Node, utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes}};

/// Creates transaction data based on the in-memory node state
pub async fn create_transaction_data(
) -> Result<(Vec<Vec<u8>>, Vec<u8>), Box<dyn std::error::Error>> {
    
    let transaction_size = 256; // Static since it's defined in TOML, update if needed
    let data_shards = 4;        // Static since it's defined in TOML, update if needed

    let transaction_data = vec![1; transaction_size];
    // info!(
    //     "Generated transaction data of size: {} bytes",
    //     transaction_data.len()
    // );

    let shards = split_into_shards(&transaction_data, data_shards);
    // info!(
    //     "Transaction data split into {} shards",
    //     data_shards
    // );

    let proofs: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| {
            let hash = Sha256::digest(shard).to_vec();
            // info!("Computed hash for shard: {:?}", hash);
            hash
        })
        .collect();

    if let Err(e) = validate_shard_sizes(&shards, transaction_size) {
        error!("Shard size validation failed: {}", e);
        return Err(e.into());
    }

    let merkle_root = compute_merkle_root(&proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    Ok((shards, merkle_root))
}
