use std::sync::Arc;
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};
use tracing::info;
use anyhow::{anyhow, Error}; // ✅ Import `anyhow::Error` and `anyhow` macro for proper error handling
use crate::{
    structs::node::Node, 
    utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes}
};

/// ✅ **Fixed: Now using `anyhow::Error` to ensure errors are `Send + Sync`**
pub async fn create_transaction_data(
    node: Arc<RwLock<Node>>, 
) -> Result<(Vec<Vec<u8>>, Vec<u8>, Vec<String>), Error> {  
    
    let node_id;
    let node_number_of_transactions: usize;
    let transaction_size: usize;
    let data_shards: usize;
    {
        let node_read = node.read().await;
        node_id = node_read.id;
        node_number_of_transactions = node_read.number_of_transactions;
        transaction_size = node_read.transaction_size;
        data_shards = node_read.data_shards;
    } // 🔴 Drop read lock immediately after fetching `id`
    info!(
        "Node {}: Creating {} transactions with {} shards and a trasnsaction size of {}",
        node_id, node_number_of_transactions, data_shards, transaction_size
    );
    let node_id: u8 = node_id.try_into().map_err(|_| anyhow!("Node ID too large"))?;
    let mut all_shards = Vec::new();
    let mut proofs = Vec::new();

    // ✅ Dynamically generate `node_number_of_transactions` transactions while ensuring batch size limits
    for _ in 0..node_number_of_transactions {
        let transaction_data = vec![node_id; transaction_size];
        let shards = split_into_shards(&transaction_data, data_shards);

        let merkle_proofs: Vec<Vec<u8>> = shards.iter()
            .map(|shard| Sha256::digest(shard).to_vec())
            .collect();
        
        validate_shard_sizes(&shards, transaction_size).map_err(Error::msg)?;

        all_shards.push(shards.clone());  // ✅ Ensure each transaction's shards are separate
        proofs.push(merkle_proofs);
    }

    let merkle_root = compute_merkle_root(&proofs.concat()); // ✅ Compute Merkle root over all shards

    // ✅ Step 1: Acquire read lock
    let node_read = node.read().await;

    // ✅ Step 2: Extract current epoch safely
    let round_id = *node_read.current_round.lock().await;

    // ✅ Step 3: Retrieve last committed parent(s) from DAG (Always use previous epoch)
    let parent_units = node_read.get_all_parents(round_id).await;
    info!(
        "Creating transaction: {} transactions, Parent Units = {:?} for Epoch {}",
        node_number_of_transactions, parent_units, round_id
    );
    info!("Created transactions shards {:?}", all_shards.concat());
    info!("Created transactions root {:?}", merkle_root);
    info!("Created transactions parents {:?}", parent_units);

    // ✅ Return only shards, merkle_root, and parents
    Ok((all_shards.concat(), merkle_root, parent_units))  // ✅ Flatten shard structure
}

