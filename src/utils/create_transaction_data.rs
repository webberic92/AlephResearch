use std::sync::Arc;
use base64::Engine;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::info;
use anyhow::Error;
use num_bigint::BigInt;
use crate::{structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}}, utils::{rsa_accumulator_util::{compute_accumulator, generate_proof}, shard_util::{split_into_shards, validate_shard_sizes}}};

/// 🧰 Pads or truncates a vector to match the given size
fn pad_to_size(mut data: Vec<u8>, size: usize) -> Vec<u8> {
    if data.len() >= size {
        data.truncate(size);
    } else {
        data.resize(size, 0);
    }
    data
}

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<ProposeRequest, Error> {  
    let (node_id, node_number_of_transactions, transaction_size, data_shards);
    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        node_number_of_transactions = node_guard.number_of_transactions;
        transaction_size = node_guard.transaction_size;
        data_shards = node_guard.data_shards;
    }

    info!(
        "Node {}: Creating {} transactions with {} shards and a transaction size of {}",
        node_id, node_number_of_transactions, data_shards, transaction_size
    );

    let mut transactions = Vec::new();

    for tx_index in 0..node_number_of_transactions {
        let content = format!("node{}_tx{}", node_id, tx_index);
        let transaction_data = pad_to_size(content.into_bytes(), transaction_size);
        
        let shards = split_into_shards(&transaction_data, data_shards);
        validate_shard_sizes(&shards, transaction_size).map_err(Error::msg)?;

        let shard_hashes: Vec<Vec<u8>> = shards.iter()
            .map(|shard| Sha256::digest(shard).to_vec())
            .collect();

        let accumulator: BigInt = compute_accumulator(&shard_hashes);
        let encoded_accumulator = base64::engine::general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

        // For each shard, generate an RSA inclusion proof
        let proofs: Vec<String> = (0..shard_hashes.len())
            .map(|i| {
                let proof = generate_proof(&shard_hashes, i, &accumulator);
                let encoded = base64::engine::general_purpose::STANDARD.encode(proof.to_bytes_be().1);
                encoded
            })
            .collect();

        let encoded_shards: Vec<String> = shards.iter()
            .map(|s| base64::engine::general_purpose::STANDARD.encode(s))
            .collect();

        transactions.push(Transaction {
            accumulator: encoded_accumulator,
            proofs: vec![proofs], // use `Vec<Vec<String>>` for compatibility
            shards: encoded_shards,
        });
    }

    let (round_id, parent_units);
    {
        let node_guard = node.lock().await;
        round_id = *node_guard.current_round.lock().await;
        parent_units = node_guard.get_all_parents(round_id).await;
    }

    info!(
        "Creating proposal: {} transactions, Parent Units = {:?} for round {}",
        node_number_of_transactions, parent_units, round_id
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
    })
}
