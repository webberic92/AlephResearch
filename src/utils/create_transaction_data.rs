use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant};
use tracing::info;
use anyhow::Error;
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{compute_accumulator, generate_proofs},
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
    let timer = Instant::now();
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
    let mut all_shard_hashes = Vec::new();
    let mut tx_shard_ranges = Vec::new(); // (start_idx, end_idx)
    let mut transactions = Vec::new();
    let mut all_shards_flat = Vec::new();

    // Step 1: Generate all shards and collect hashes
    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.into_bytes(), transaction_size);
        let tx_root = Sha256::digest(&padded).to_vec();

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)?;
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

        let mut shards = data_chunks.clone();
        while shards.len() < total_nodes {
            shards.push(vec![0u8; shard_size]);
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        rs.encode(&mut shard_refs)?;

        let shard_hashes: Vec<Vec<u8>> = shards
            .iter()
            .map(|s| Sha256::digest(s).to_vec())
            .collect();

        let start = all_shard_hashes.len();
        all_shard_hashes.extend(shard_hashes.clone());
        let end = all_shard_hashes.len();
        tx_shard_ranges.push((start, end));
        all_shards_flat.extend(shards);
        
        transactions.push(Transaction {
            root: tx_root,
            shards: vec![], // Fill later
            shard_hashes: Some(shard_hashes.iter().map(hex::encode).collect()),
            accumulator: None, // Batch accumulator only
        });
    }

    // Step 2: Compute batch accumulator and batch proofs
    let accumulator = compute_accumulator(&all_shard_hashes);
    let proofs = generate_proofs(&all_shard_hashes);
    let encoded_accumulator = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    // Step 3: Assign shard/proof to each transaction
    let mut proof_idx = 0;
    for (tx_index, tx) in transactions.iter_mut().enumerate() {
        let mut shard_structs = Vec::with_capacity(total_nodes);
        for i in 0..total_nodes {
            let shard_b64 = general_purpose::STANDARD.encode(&all_shards_flat[proof_idx]);
            let proof_b64 = general_purpose::STANDARD.encode(proofs[proof_idx].to_bytes_be().1);
            shard_structs.push(ShardWithProofs {
                shard_b64,
                proofs: vec![proof_b64],
            });
            proof_idx += 1;
        }
        tx.shards = shard_structs;
        tx.accumulator = Some(encoded_accumulator.clone()); // Set for consistency
    }
    let elapsed = timer.elapsed();
    tracing::info!(
        "📦 create_transaction_data(): Completed {} txs in {:.2?} (avg: {:.2?} per tx)",
        num_txs,
        elapsed,
        elapsed / num_txs as u32
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
