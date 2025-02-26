use std::sync::Arc;
use base64::Engine;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::info;
use anyhow::{anyhow, Error};
use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}}, 
    utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, split_into_shards, validate_shard_sizes}
};

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<ProposeRequest, Error> {  
    info!("🔍 [DEBUG] Attempting to acquire node lock for create_transaction_data()");

    // ✅ Extract necessary values as quickly as possible and then **drop** the lock
    let (node_id, node_number_of_transactions, transaction_size, data_shards);
    {
        info!("🔍 [DEBUG] Waiting to acquire node lock for create_transaction_data()");
        let node_guard = node.lock().await;
        info!("🔓 [DEBUG] Acquired node lock for create_transaction_data()");
        
        node_id = node_guard.id;
        node_number_of_transactions = node_guard.number_of_transactions;
        transaction_size = node_guard.transaction_size;
        data_shards = node_guard.data_shards;
    } // 🔥 **Lock is dropped here automatically!**

    info!(
        "Node {}: Creating {} transactions with {} shards and a transaction size of {}",
        node_id, node_number_of_transactions, data_shards, transaction_size
    );

    let mut transactions = Vec::new();

    for _ in 0..node_number_of_transactions {
        let transaction_data = vec![node_id as u8; transaction_size]; // ✅ Convert safely
        let shards = split_into_shards(&transaction_data, data_shards);

        let shard_hashes: Vec<Vec<u8>> = shards.iter()
            .map(|shard| Sha256::digest(shard).to_vec())  // ✅ Hash each shard
            .collect();
        
        let merkle_root = compute_merkle_root(&shard_hashes); // ✅ Compute root from hashed shards

        let proofs: Vec<Vec<Vec<u8>>> = shard_hashes
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(&shard_hashes, i))  // ✅ Generate correct Merkle proof
            .collect();

        validate_shard_sizes(&shards, transaction_size).map_err(Error::msg)?;

        let encoded_shards: Vec<String> = shards.iter()
            .map(|s| base64::engine::general_purpose::STANDARD.encode(s))
            .collect();

        let encoded_proofs: Vec<Vec<String>> = proofs.iter()
            .map(|proof| proof.iter()
                .map(|p| base64::engine::general_purpose::STANDARD.encode(p))
                .collect()
            ).collect();

        transactions.push(Transaction {
            root: merkle_root,  
            proofs: encoded_proofs,
            shards: encoded_shards,
        });
    }

    // ✅ **Reacquire the lock only when needed**
    info!("🔍 [DEBUG] Waiting to acquire node lock to retrieve round_id");
    let (round_id, parent_units);
    {
        let node_guard = node.lock().await;
        info!("🔓 [DEBUG] Acquired node lock to retrieve round_id");
        round_id = *node_guard.current_round.lock().await;
        parent_units = node_guard.get_all_parents(round_id).await;
    } // 🔥 **Lock is dropped again here**

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


