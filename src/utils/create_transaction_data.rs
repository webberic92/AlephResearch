use std::sync::Arc;
use base64::{engine::general_purpose, Engine};
use futures::future::join_all;
use num_bigint::BigInt;
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::info;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{compute_accumulator_from_primes, memoized_hash_to_prime},
};

pub fn pad_to_len(mut data: Vec<u8>, target_len: usize) -> Vec<u8> {
    if data.len() >= target_len {
        data.truncate(target_len);
    } else {
        data.resize(target_len, 0);
    }
    data
}

pub async fn create_transaction_data(node: Arc<Mutex<Node>>) -> Result<ProposeRequest, anyhow::Error> {
    // Pull config from node
    let (node_id, num_txs, data_shards, total_nodes, transaction_size) = {
        let node_guard = node.lock().await;
        (
            node_guard.id,
            node_guard.number_of_transactions,
            node_guard.data_shards,
            node_guard.total_nodes,
            node_guard.transaction_size,
        )
    };

    // Get round_id
    let round_id = {
        let node_guard = node.lock().await;
        let current_round = node_guard.current_round.lock().await;
        *current_round
    };

    // Get parent units
    let parent_units = {
        let node_guard = node.lock().await;
        node_guard.get_all_parents(round_id).await
    };

    let parent_units_bytes: Vec<Vec<u8>> = parent_units.into_iter().map(|s| s.into_bytes()).collect();
    let parent_hash = Sha256::digest(&parent_units_bytes.concat());

    let mut transactions: Vec<Transaction> = Vec::with_capacity(num_txs);

    // Generate transactions
    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let mut tx_data = content.clone().into_bytes();
        tx_data.extend(&parent_hash);
        let padded_data = pad_to_len(tx_data, transaction_size);

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)?;
        let mut shards: Vec<Vec<u8>> = padded_data
            .chunks((transaction_size + data_shards - 1) / data_shards)
            .map(|chunk| {
                let mut v = chunk.to_vec();
                v.resize((transaction_size + data_shards - 1) / data_shards, 0);
                v
            })
            .collect();

        while shards.len() < total_nodes {
            shards.push(vec![0u8; (transaction_size + data_shards - 1) / data_shards]);
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        rs.encode(&mut shard_refs)?;

        let shard_hashes: Vec<String> = shards[..data_shards]
            .iter()
            .map(|shard| hex::encode(Sha256::digest(shard)))
            .collect();

        let shard_structs: Vec<ShardWithProofs> = shards
            .into_iter()
            .map(|shard| ShardWithProofs {
                shard_b64: general_purpose::STANDARD.encode(&shard),
            })
            .collect();

        transactions.push(Transaction {
            accumulator: String::new(),  // <-- Initially empty
            shards: shard_structs,
            shard_hashes,
            number_of_data_shards: data_shards,
        });
    }

    // Canonical deterministic ordering before flattening shard hashes
    transactions.sort_by_key(|tx| {
        let first_shard_hash = &tx.shard_hashes[0];
        first_shard_hash.clone()
    });

    let mut all_shard_hashes: Vec<String> = Vec::new();
    for tx in &transactions {
        all_shard_hashes.extend(tx.shard_hashes.clone());
    }

    all_shard_hashes.sort_unstable();
    all_shard_hashes.dedup();

    info!("Node {} Round {}: Propose phase - All shard hashes used for batch accumulator:", node_id, round_id);
    // for (idx, hash_hex) in all_shard_hashes.iter().enumerate() {
    //     info!("Shard [{}]: {}", idx, hash_hex);
    // }

    let prime_futures = all_shard_hashes
        .iter()
        .map(|hex_str| memoized_hash_to_prime(hex_str))
        .collect::<Vec<_>>();

    let all_primes: Vec<BigInt> = join_all(prime_futures).await;
    let batch_acc = compute_accumulator_from_primes(&all_primes);
    let batch_acc_b64 = general_purpose::STANDARD.encode(batch_acc.to_bytes_be().1);

    // ✅ Inject batch_accumulator into each Transaction
    for tx in &mut transactions {
        tx.accumulator = batch_acc_b64.clone();
    }

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units_bytes,
        batch_accumulator: batch_acc_b64,
    })
}

