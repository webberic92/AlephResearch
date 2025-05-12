use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use num_bigint::BigInt;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant, task::spawn_blocking};
use tracing::info;
use anyhow::Error;
use reed_solomon_erasure::galois_8::ReedSolomon;
use rayon::prelude::*;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{compute_accumulator_radix, generate_proofs_radix},
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
) -> Result<ProposeRequest, anyhow::Error> {
    let timer = std::time::Instant::now();
    info!("📦 create_transaction_data(): Starting transaction data creation...");
    let (node_id, num_txs, data_shards, total_nodes, transaction_size, round_id, parent_units) = {
        let node_guard = node.lock().await;
        // info!(
        //     "Node ID: {}, Number of Transactions: {}, Data Shards: {}, Total Nodes: {}, Transaction Size: {}",
        //     node_guard.id,
        //     node_guard.number_of_transactions,
        //     node_guard.data_shards,
        //     node_guard.total_nodes,
        //     node_guard.transaction_size
        // );
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

    let mut transactions = Vec::with_capacity(num_txs);
    let mut all_data_hashes = Vec::with_capacity(num_txs * data_shards);
    let mut all_shards = Vec::new();
    let mut all_hashes_per_tx = Vec::with_capacity(num_txs);
    
    info!("Shard size: {}", shard_size);
    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.into_bytes(), transaction_size);
        // let tx_root = Sha256::digest(&padded).to_vec();
        // info!("tx_index: {}, padded len: {}", tx_index, padded.len());
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

        let shard_hashes: Vec<Vec<u8>> = shards.iter()
            .take(data_shards)
            .map(|s| Sha256::digest(s).to_vec())
            .collect();

        all_data_hashes.extend_from_slice(&shard_hashes);
        all_shards.push(shards);
        all_hashes_per_tx.push(shard_hashes);
    }

    let (accumulator, proofs) = spawn_blocking(move || {
        let acc = compute_accumulator_radix(&all_data_hashes);
        let proofs = generate_proofs_radix(&all_data_hashes);
        (acc, proofs)
    }).await?;

    let encoded_acc = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    let mut proof_idx = 0;
    for (tx_index, shards) in all_shards.into_iter().enumerate() {
        let mut shard_structs = Vec::with_capacity(total_nodes);
        for shard_i in 0..total_nodes {
            let shard_b64 = general_purpose::STANDARD.encode(&shards[shard_i]);

            let proofs_vec = if shard_i < data_shards {
                vec![general_purpose::STANDARD.encode(
                    proofs[proof_idx].to_bytes_be().1,
                )]
            } else {
                vec![]
            };

            if shard_i < data_shards {
                proof_idx += 1;
            }

            shard_structs.push(ShardWithProofs {
                shard_b64,
                proofs: proofs_vec,
            });
        }

        // ✅ NEW: encode the hashes used for proof verification
        let encoded_hashes: Vec<String> = all_hashes_per_tx[tx_index]
            .iter()
            .map(|h| hex::encode(h))
            .collect();

        transactions.push(Transaction {
            root: Sha256::digest(&pad_to_len(
                format!("tx{}_round{}", tx_index + 1, round_id).into_bytes(),
                transaction_size,
            ))
            .to_vec(),
            shards: shard_structs,
            accumulator: Some(encoded_acc.clone()),
            shard_hashes: Some(encoded_hashes), // ✅ add this line
        });
    }

    tracing::info!(
        "📦 create_transaction_data(): Completed {} txs in {:.2?} (avg: {:.2?} per tx)",
        num_txs,
        timer.elapsed(),
        timer.elapsed() / num_txs as u32
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_accumulator: encoded_acc,
    })
}

