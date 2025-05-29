use std::sync::Arc;

use base64::{engine::general_purpose, Engine};
use rayon::prelude::*;
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
    utils::rsa_accumulator_util::{
        compute_accumulator_from_primes,
        generate_proofs_from_primes_radix,
        hash_to_prime_128,
    },
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
    info!("\u{1f4e6} Starting transaction data creation...");

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

    let transactions: Vec<_> = (0..num_txs).into_par_iter().map(|tx_index| {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.clone().into_bytes(), transaction_size);

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards).unwrap();
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
        rs.encode(&mut shard_refs).unwrap();

        // Step 1: Hash data shards and compute primes
        let tx_hashes: Vec<Vec<u8>> = shards[..data_shards]
            .iter()
            .map(|s| Sha256::digest(s).to_vec())
            .collect();

        let tx_primes: Vec<BigInt> = tx_hashes
            .iter()
            .map(|hash| hash_to_prime_128(hash))
            .collect();

        // Step 2: Compute accumulator from all primes
        let tx_accumulator = compute_accumulator_from_primes(&tx_primes);
        let tx_acc_encoded = general_purpose::STANDARD.encode(tx_accumulator.to_bytes_be().1);

        // Step 3: Generate proofs for each prime against the accumulator
        let proofs = generate_proofs_from_primes_radix(&tx_primes);
        let encoded_proofs: Vec<String> = proofs
            .iter()
            .map(|p| general_purpose::STANDARD.encode(p.to_bytes_be().1))
            .collect();

        // Step 4: Build shard structs, attaching proofs to data shards
        let mut shard_structs = Vec::with_capacity(total_nodes);
        for shard_i in 0..total_nodes {
            let shard_b64 = general_purpose::STANDARD.encode(&shards[shard_i]);

            let proofs_vec = if shard_i < data_shards {
                vec![encoded_proofs[shard_i].clone()]
            } else {
                vec![]
            };

            shard_structs.push(ShardWithProofs {
                shard_b64,
                proofs: proofs_vec,
            });
        }

        // Step 5: Finalize transaction
        let shard_hashes_hex: Vec<String> = tx_hashes.iter().map(|h| hex::encode(h)).collect();
        let padded_root = pad_to_len(content.into_bytes(), transaction_size);

        Transaction {
            root: Sha256::digest(&padded_root).to_vec(),
            shards: shard_structs,
            accumulator: Some(tx_acc_encoded),
            shard_hashes: Some(shard_hashes_hex),
        }
    }).collect();

    info!("\u{1f4dd} Created {} transactions in {:?}", num_txs, timer.elapsed());

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_accumulator: "".to_string(),
    })
}


// /// Dynamically adjusts the number of data shards based on total nodes and number of transactions.
// /// Ensures enough redundancy (i.e., tolerating up to f Byzantine nodes if data_shards = N - f).
// pub async fn maybe_adjust_shard_count(node: Arc<Mutex<Node>>) {
//     let mut node_guard = node.lock().await;

//     let total_nodes = node_guard.total_nodes;
//     let num_txs = node_guard.number_of_transactions;

//     // Aim for up to f = N / 3 fault tolerance: data_shards = N - f
//     let optimal_data_shards = std::cmp::max(
//         1,
//         std::cmp::min(num_txs, total_nodes - total_nodes / 3),
//     );

//     if node_guard.data_shards != optimal_data_shards {
//         tracing::info!(
//             "Adjusting data_shards: {} → {} based on N = {}, txs = {}",
//             node_guard.data_shards,
//             optimal_data_shards,
//             total_nodes,
//             num_txs
//         );
//         node_guard.data_shards = optimal_data_shards;
//     } else {
//         tracing::info!(
//             "data_shards already optimal ({}); no update needed.",
//             node_guard.data_shards
//         );
//     }
// }
