use std::sync::Arc;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::info;
use anyhow::{anyhow, Error}; // ✅ Import `anyhow::Error` and `anyhow` macro for proper error handling
use crate::{
    structs::node::Node, 
    utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes}
};

/// ✅ **Updated: Now using `Arc<Mutex<Node>>` for consistency with new architecture**
pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<(Vec<Vec<u8>>, Vec<u8>, Vec<String>), Error> {  
    
    let node_id;
    let node_number_of_transactions: usize;
    let transaction_size: usize;
    let data_shards: usize;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        node_number_of_transactions = node_guard.number_of_transactions;
        transaction_size = node_guard.transaction_size;
        data_shards = node_guard.data_shards;
    } // 🔴 Drop lock immediately after fetching necessary fields

    info!(
        "Node {}: Creating {} transactions with {} shards and a transaction size of {}",
        node_id, node_number_of_transactions, data_shards, transaction_size
    );

    let node_id: u8 = node_id.try_into().map_err(|_| anyhow!("Node ID too large"))?;
    let mut all_shards = Vec::new();
    let mut proofs = Vec::new();

    // ✅ Dynamically generate transactions
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

    // ✅ Step 1: Re-acquire lock to get round information and parents
    let node_guard = node.lock().await;

    // ✅ Step 2: Extract current round safely
    let round_id = *node_guard.current_round.lock().await;

    // ✅ Step 3: Retrieve last committed parent(s) from DAG (Always use previous round)
    let parent_units = node_guard.get_all_parents(round_id).await;

    info!(
        "Creating transaction: {} transactions, Parent Units = {:?} for round {}",
        node_number_of_transactions, parent_units, round_id
    );
    info!("Created transaction shards: {:?}", all_shards.concat());
    info!("Created transaction root: {:?}", merkle_root);
    info!("Created transaction parents: {:?}", parent_units);

    // ✅ Return only shards, merkle_root, and parents
    Ok((all_shards.concat(), merkle_root, parent_units))  // ✅ Flatten shard structure
}
