use std::sync::Arc;
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use anyhow::Error; // ✅ Import `anyhow::Error` for proper error handling
use crate::{
    structs::node::Node, 
    utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes}
};

/// ✅ **Fixed: Now using `anyhow::Error` to ensure errors are `Send + Sync`**
pub async fn create_transaction_data() -> Result<(Vec<Vec<u8>>, Vec<u8>), Error> {
    
    let transaction_size = 256; // Static since it's defined in TOML, update if needed
    let data_shards = 4;        // Static since it's defined in TOML, update if needed

    let transaction_data = vec![1; transaction_size];

    let shards = split_into_shards(&transaction_data, data_shards);

    let proofs: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    // ✅ **Error propagation using `map_err(Error::msg)` to ensure compatibility**
    validate_shard_sizes(&shards, transaction_size)
        .map_err(Error::msg)?;

    let merkle_root = compute_merkle_root(&proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    Ok((shards, merkle_root))
}
