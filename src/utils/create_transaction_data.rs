use std::num::NonZero;
use std::sync::Arc;

use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant};
use tracing::info;
use reed_solomon_erasure::galois_8::ReedSolomon;
use num_bigint::BigInt;
use rayon::prelude::*;
use lru::LruCache;
use once_cell::sync::Lazy;
use std::sync::Mutex as StdMutex;

use crate::{
    structs::{
        node::Node,
        requests::{BaseRequest, ProposeRequest, ShardWithProofs, Transaction},
    },
    utils::rsa_accumulator_util::{
        compute_accumulator_from_primes,
        generate_proofs_from_primes_radix, memoized_hash_to_prime,
    },
};

static PROOF_CACHE: Lazy<StdMutex<LruCache<u64, BigInt>>> =
    Lazy::new(|| StdMutex::new(LruCache::new(NonZero::new(10000).unwrap())));

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
    let parent_hash = Sha256::digest(&parent_units.concat());

    let mut all_primes = Vec::with_capacity(num_txs * data_shards);
    let mut transactions = Vec::with_capacity(num_txs);

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let mut tx_data = content.clone().into_bytes();
        tx_data.extend(&parent_hash); // ⛓️ Chain parent hashes
        let padded_data = pad_to_len(tx_data, transaction_size);

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)?;
        let mut shards: Vec<Vec<u8>> = padded_data
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

        let hash_hexes: Vec<String> = shards[..data_shards]
            .par_iter()
            .map(|shard| hex::encode(Sha256::digest(shard)))
            .collect();

        let mut tx_primes = Vec::with_capacity(data_shards);
        for hash_hex in &hash_hexes {
            let prime = memoized_hash_to_prime(hash_hex).await;
            tx_primes.push(prime.clone());
            all_primes.push(prime);
        }

        let accumulator = compute_accumulator_from_primes(&tx_primes);
        let acc_encoded = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

        let proofs = generate_proofs_from_primes_radix(&tx_primes);
        let encoded_proofs: Vec<String> = proofs
            .iter()
            .map(|p| general_purpose::STANDARD.encode(p.to_bytes_be().1))
            .collect();

        let mut shard_structs = Vec::with_capacity(total_nodes);
        for shard_i in 0..total_nodes {
            let shard_b64 = general_purpose::STANDARD.encode(&shards[shard_i]);
            let proofs_vec = if shard_i < data_shards && shard_i < encoded_proofs.len() {
                vec![encoded_proofs[shard_i].clone()]
            } else {
                vec![] // Parity shard — no proof
            };
            
            shard_structs.push(ShardWithProofs {
                shard_b64,
                proofs: proofs_vec,
            });
        }

        let root = Sha256::digest(&padded_data).to_vec();
        transactions.push(Transaction {
            root,
            shards: shard_structs,
            accumulator: Some(acc_encoded),
            shard_hashes: Some(hash_hexes),
            number_of_data_shards: data_shards, // ✅ <- Added this line
        });
    }

    let batch_acc = compute_accumulator_from_primes(&all_primes);
    let batch_acc_b64 = general_purpose::STANDARD.encode(batch_acc.to_bytes_be().1);

    info!("📝 Created {} transactions in {:?}", num_txs, timer.elapsed());

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_accumulator: batch_acc_b64,
    })
}



