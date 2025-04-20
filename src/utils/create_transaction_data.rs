use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::{info, error};
use anyhow::Error;
use num_bigint::BigInt;
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}},
    utils::rsa_accumulator_util::{compute_accumulator, generate_proof}
};

pub fn pad_to_len(mut data: Vec<u8>, target_len: usize) -> Vec<u8> {
    if data.len() >= target_len {
        data.truncate(target_len);
    } else {
        data.resize(target_len, 0);
    }
    data
}

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>,
) -> Result<ProposeRequest, Error> {
    let (node_id, num_txs, data_shards, total_nodes, transaction_size, round_id, parent_units) = {
        let node_guard = node.lock().await;
        let round_id = *node_guard.current_round.lock().await;
        let parent_units = node_guard
            .get_all_parents(round_id)
            .await
            .into_iter()
            .map(|s| s.into_bytes())
            .collect::<Vec<_>>();

        (
            node_guard.id,
            node_guard.number_of_transactions,
            node_guard.data_shards,
            node_guard.total_nodes,
            node_guard.transaction_size,
            round_id,
            parent_units,
        )
    };

    let shard_size = (transaction_size + data_shards - 1) / data_shards;
    let mut all_shard_hashes = Vec::new(); // for accumulator
    let mut shard_hash_index_map = Vec::new(); // to track which hashes belong to which tx

    let mut transactions = Vec::new();

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.into_bytes(), transaction_size);
        let tx_root = Sha256::digest(&padded).to_vec();

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)
            .map_err(|e| Error::msg(format!("RS init failed: {:?}", e)))?;

        // Prepare data shards
        let mut data_chunks: Vec<Vec<u8>> = padded
            .chunks(shard_size)
            .map(|chunk| {
                let mut v = chunk.to_vec();
                v.resize(shard_size, 0);
                v
            })
            .collect();
        while data_chunks.len() < data_shards {
            data_chunks.push(vec![0u8; shard_size]);
        }

        // Add parity shards
        let mut shards = data_chunks.clone();
        while shards.len() < total_nodes {
            shards.push(vec![0u8; shard_size]);
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        rs.encode(&mut shard_refs)
            .map_err(|e| Error::msg(format!("RS encoding failed: {:?}", e)))?;

        // Encode and hash shards
        let mut shard_hashes_this_tx = Vec::new();
        let encoded_shards: Vec<String> = shards
            .iter()
            .map(|shard| {
                let hash = Sha256::digest(shard).to_vec();
                all_shard_hashes.push(hash.clone());
                shard_hashes_this_tx.push(hash);
                general_purpose::STANDARD.encode(shard)
            })
            .collect();

        shard_hash_index_map.push(shard_hashes_this_tx);

        // We'll attach proofs later
        transactions.push(Transaction {
            root: tx_root,
            shards: encoded_shards,
            proofs: vec![], // temporarily empty
        });
    }

    // Build the accumulator
    let accumulator = compute_accumulator(&all_shard_hashes);
    let encoded_accumulator = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    // Now compute proofs per shard
    let mut flat_proofs = Vec::new();
    for (i, _) in all_shard_hashes.iter().enumerate() {
        let proof = generate_proof(&all_shard_hashes, i, &accumulator);
        flat_proofs.push(general_purpose::STANDARD.encode(proof.to_bytes_be().1));
    }

    // Assign per-tx proof slices to transactions
    let mut cursor = 0;
    for (tx, hashes_for_tx) in transactions.iter_mut().zip(shard_hash_index_map.iter()) {
        let proofs_for_tx: Vec<String> = flat_proofs[cursor..cursor + hashes_for_tx.len()].to_vec();
        tx.proofs = proofs_for_tx;
        cursor += hashes_for_tx.len();
    }

    info!(
        "✅ RSA-based proposal ready: {} txs, {} total shards, Round {}, Accumulator: {}...",
        num_txs,
        all_shard_hashes.len(),
        round_id,
        &encoded_accumulator[..12]
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_accumulator: encoded_accumulator,
    })
}


