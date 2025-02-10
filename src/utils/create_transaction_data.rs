use std::sync::Arc;
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use anyhow::{anyhow, Error}; // ✅ Import `anyhow::Error` and `anyhow` macro for proper error handling
use crate::{
    structs::node::Node, 
    utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes}
};

/// ✅ **Fixed: Now using `anyhow::Error` to ensure errors are `Send + Sync`**
pub async fn create_transaction_data(
    node: Arc<RwLock<Node>>, 
    id: usize
) -> Result<(Vec<Vec<u8>>, Vec<u8>, Vec<String>), Error> {
    let transaction_size = 256;
    let data_shards = 4;

    let node_id: u8 = id.try_into().map_err(|_| anyhow!("Node ID too large"))?;
    let transaction_data = vec![node_id; transaction_size];

    let shards = split_into_shards(&transaction_data, data_shards);
    let proofs: Vec<Vec<u8>> = shards.iter().map(|shard| Sha256::digest(shard).to_vec()).collect();
    
    validate_shard_sizes(&shards, transaction_size).map_err(Error::msg)?;

    let merkle_root = compute_merkle_root(&proofs);

    // ✅ Step 1: Acquire read lock
    let node_read = node.read().await;

    // ✅ Step 2: Extract current epoch safely
    let epoch_id = *node_read.current_epoch.lock().await;

    // ✅ Step 3: Retrieve last committed parent(s) from DAG (Always use previous epoch)
    let parent_units = node_read.get_all_parents(epoch_id).await;
    info!("Creating transaction: Parent Units = {:?} for Epoch {}", parent_units, epoch_id);

    Ok((shards, merkle_root, parent_units))
}
