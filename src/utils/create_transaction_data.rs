use std::sync::Arc;
use base64::Engine;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::info;
use anyhow::{anyhow, Error};
use crate::{
    structs::node::Node, 
    utils::merkle_utils::{compute_merkle_root, split_into_shards, validate_shard_sizes},
    structs::requests::{BaseRequest, ProposeRequest, Transaction}
};

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<ProposeRequest, Error> {  

    let node_id: usize;
    let node_number_of_transactions: usize;
    let transaction_size: usize;
    let data_shards: usize;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;  // `usize` for Rust logic
        node_number_of_transactions = node_guard.number_of_transactions;
        transaction_size = node_guard.transaction_size;
        data_shards = node_guard.data_shards;
    }

    info!(
        "Node {}: Creating {} transactions with {} shards and a transaction size of {}",
        node_id, node_number_of_transactions, data_shards, transaction_size
    );

    let mut transactions = Vec::new();

    for _ in 0..node_number_of_transactions {
        let transaction_data = vec![node_id as u8; transaction_size]; // ✅ Convert here safely
        let shards = split_into_shards(&transaction_data, data_shards);

        let merkle_proofs: Vec<Vec<u8>> = shards.iter()
            .map(|shard| Sha256::digest(shard).to_vec())
            .collect();
        
        validate_shard_sizes(&shards, transaction_size).map_err(Error::msg)?;

        let encoded_shards: Vec<String> = shards.iter()
            .map(|s| base64::engine::general_purpose::STANDARD.encode(s))
            .collect();

        let encoded_proofs: Vec<Vec<String>> = merkle_proofs.iter()
            .map(|proof| vec![base64::engine::general_purpose::STANDARD.encode(proof)]) 
            .collect();

        let merkle_root = compute_merkle_root(&merkle_proofs); // ✅ This is Vec<u8>

        transactions.push(Transaction {
            root: merkle_root,  
            proofs: encoded_proofs,
            shards: encoded_shards,
        });
    }

    let node_guard = node.lock().await;
    let round_id = *node_guard.current_round.lock().await;
    let parent_units = node_guard.get_all_parents(round_id).await;

    info!(
        "Creating proposal: {} transactions, Parent Units = {:?} for round {}",
        node_number_of_transactions, parent_units, round_id
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,  // ✅ Safe conversion
            round_id,
        },
        transactions,
        parents: parent_units,
    })
}
