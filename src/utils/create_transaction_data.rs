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
    utils::{rsa_accumulator_util::{compute_accumulator, generate_proof}, shard_util::split_into_shards}
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
    let mut shard_hashes = Vec::new();
    let mut all_encoded_shards = Vec::new();
    let mut transactions = Vec::new();

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.into_bytes(), transaction_size);

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)
            .map_err(|e| Error::msg(format!("RS init failed: {:?}", e)))?;

        let mut data_chunks = vec![];
        for i in 0..data_shards {
            let start = i * shard_size;
            let end = std::cmp::min(start + shard_size, padded.len());
            let mut chunk = padded[start..end].to_vec();
            chunk.resize(shard_size, 0);
            data_chunks.push(chunk);
        }

        let mut shards = data_chunks.clone();
        while shards.len() < total_nodes {
            shards.push(vec![0u8; shard_size]);
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        rs.encode(&mut shard_refs)
            .map_err(|e| Error::msg(format!("RS encoding failed: {:?}", e)))?;

        let encoded_shards: Vec<String> = shards
            .iter()
            .map(|shard| {
                let hash = Sha256::digest(shard).to_vec();
                shard_hashes.push(hash.clone());
                general_purpose::STANDARD.encode(shard)
            })
            .collect();

        all_encoded_shards.push(encoded_shards.clone());

        transactions.push(Transaction {
            shards: encoded_shards,
        });
    }

    // ✅ Compute accumulator over all shard hashes
    let accumulator: BigInt = compute_accumulator(&shard_hashes);
    let encoded_accumulator = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    // ✅ Generate inclusion proofs per shard hash
    let batch_proofs: Vec<Vec<String>> = shard_hashes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let proof = generate_proof(&shard_hashes, i, &accumulator);
            vec![general_purpose::STANDARD.encode(proof.to_bytes_be().1)]
        })
        .collect();

    info!(
        "✅ RSA-based proposal ready: {} txs, {} total shards, Round {}, Accumulator: {}",
        num_txs,
        shard_hashes.len(),
        round_id,
        &encoded_accumulator[..12] // shortened preview
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_accumulator: encoded_accumulator,
        batch_proofs,
    })
}
