use std::sync::Arc;
use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, task::spawn_blocking, time::Instant};
use tracing::info;
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{compute_accumulator_radix, generate_proofs_radix, hash_to_prime_128},
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
    const GROUP_SIZE: usize = 8;
    let timer = Instant::now();
    info!("📦 Starting transaction data creation...");

    let (node_id, num_txs, data_shards, total_nodes, transaction_size, round_id, parent_units) = {
        let node_guard = node.lock().await;
        let round_id = *node_guard.current_round.lock().await;
        let parent_units = node_guard.get_all_parents(round_id)
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

    for (group_id, chunk) in (0..num_txs).collect::<Vec<_>>().chunks(GROUP_SIZE).enumerate() {
        let mut group_data_hashes = Vec::new();
        let mut tx_hashes = Vec::new();
        let mut tx_shard_b64s = Vec::new();

        for &tx_index in chunk {
            let content = format!("tx{}_round{}", tx_index + 1, round_id);
            let padded = pad_to_len(content.clone().into_bytes(), transaction_size);
            info!("🧬 tx[{}] padded: {:?}", tx_index, String::from_utf8_lossy(&padded));

            let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)?;
            let mut shards: Vec<Vec<u8>> = padded
                .chunks(shard_size)
                .map(|chunk| {
                    let mut v = chunk.to_vec();
                    v.resize(shard_size, 0);
                    v
                })
                .collect();

            while shards.len() < total_nodes {
                shards.push(vec![0u8; shard_size]);
            }

            let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
            rs.encode(&mut shard_refs)?;

            let mut data_hashes = Vec::new();
            let mut shard_b64s = Vec::new();
            for (i, s) in shards.iter().take(data_shards).enumerate() {
                let b64 = general_purpose::STANDARD.encode(s);
                let hash = Sha256::digest(s).to_vec();
                let prime = hash_to_prime_128(&hash);
                info!(
                    "🔐 tx[{}] shard[{}] base64-hash = {}, prime = {}",
                    tx_index,
                    i,
                    hex::encode(&hash),
                    prime
                );
                data_hashes.push(hash);
                shard_b64s.push(b64);
            }

            group_data_hashes.extend(data_hashes.clone());
            tx_hashes.push(data_hashes);
            tx_shard_b64s.push((tx_index, shards, shard_b64s));
        }

        let (accumulator, proofs) = spawn_blocking(move || {
            let acc = compute_accumulator_radix(&group_data_hashes);
            let proofs = generate_proofs_radix(&group_data_hashes);
            (acc, proofs)
        }).await?;

        info!("📍 Group[{}] accumulator = {}", group_id, hex::encode(accumulator.to_bytes_be().1));
        let encoded_acc = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

        let mut proof_idx = 0;
        for (tx_i, (tx_index, shards, shard_b64s)) in tx_shard_b64s.into_iter().enumerate() {
            let mut shard_structs = Vec::with_capacity(total_nodes);

            for shard_i in 0..total_nodes {
                let shard_b64 = if shard_i < data_shards {
                    shard_b64s[shard_i].clone()
                } else {
                    general_purpose::STANDARD.encode(&shards[shard_i])
                };

                let proofs_vec = if shard_i < data_shards {
                    let proof = proofs.get(proof_idx).expect("Missing proof");
                    let proof_b64 = general_purpose::STANDARD.encode(proof.to_bytes_be().1.clone());
                    info!(
                        "📎 Proof for tx[{}] shard[{}]: {}",
                        tx_index, shard_i, &proof_b64
                    );
                    proof_idx += 1;
                    vec![proof_b64]
                } else {
                    vec![]
                };

                shard_structs.push(ShardWithProofs {
                    shard_b64,
                    proofs: proofs_vec,
                });
            }

            let encoded_hashes: Vec<String> = tx_hashes[tx_i]
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
                accumulator: Some(encoded_acc.clone()), // ✅ per-group, not global
                accumulator_group_id: Some(group_id),
                shard_hashes: Some(encoded_hashes),
            });
        }
    }

    info!(
        "📦 Finished creating {} txs in {:.2?} (avg: {:.2?} per tx)",
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
        batch_accumulator: "".into(), // 🛑 No global accumulator
    })
}

