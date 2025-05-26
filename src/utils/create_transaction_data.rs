use std::sync::Arc;

use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant};
use tracing::info;
use reed_solomon_erasure::galois_8::ReedSolomon;
use num_bigint::BigInt;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{compute_accumulator_from_primes, generate_proofs_from_primes_radix, hash_to_prime_128},
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
    let mut all_shards = Vec::new();
    let mut all_hashes = Vec::new();
    let mut shard_hashes_per_tx = Vec::with_capacity(num_txs);

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.clone().into_bytes(), transaction_size);

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

        let mut tx_hashes = Vec::with_capacity(data_shards);
        for s in &shards[..data_shards] {
            let hash = Sha256::digest(s).to_vec();
            all_hashes.push(hash.clone());
            tx_hashes.push(hash);
        }

        all_shards.push(shards);
        shard_hashes_per_tx.push(tx_hashes);
    }

    // Compute accumulator and radix-based proofs
    info!("🧮 Computing RSA accumulator and radix proofs...");
    let all_primes: Vec<BigInt> = all_hashes.iter().map(|h| hash_to_prime_128(h)).collect();
    let accumulator = compute_accumulator_from_primes(&all_primes);
    let proofs = generate_proofs_from_primes_radix(&all_primes);
    info!("✅ RSA accumulator and radix-style proofs computed.");

    let encoded_acc = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);
    let mut proof_index = 0;

    for (tx_index, shards) in all_shards.into_iter().enumerate() {
        let mut shard_structs = Vec::with_capacity(total_nodes);
        for shard_i in 0..total_nodes {
            let shard_b64 = general_purpose::STANDARD.encode(&shards[shard_i]);

            let proofs_vec = if shard_i < data_shards {
                let proof = &proofs[proof_index];
                let proof_b64 = general_purpose::STANDARD.encode(proof.to_bytes_be().1.clone());
                proof_index += 1;
                vec![proof_b64]
            } else {
                vec![]
            };

            shard_structs.push(ShardWithProofs {
                shard_b64,
                proofs: proofs_vec,
            });
        }

        let shard_hashes_hex: Vec<String> = shard_hashes_per_tx[tx_index]
            .iter()
            .map(|h| hex::encode(h))
            .collect();

        let padded_root = pad_to_len(
            format!("tx{}_round{}", tx_index + 1, round_id).into_bytes(),
            transaction_size,
        );

        transactions.push(Transaction {
            root: Sha256::digest(&padded_root).to_vec(),
            shards: shard_structs,
            accumulator: Some(encoded_acc.clone()),
            shard_hashes: Some(shard_hashes_hex),
        });
    }

    info!("📝 Created {} transactions in {:?}", num_txs, timer.elapsed());

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
