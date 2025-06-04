use std::num::NonZero;
use std::sync::Arc;

use base64::{engine::general_purpose, Engine};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::Instant};
use tracing::info;
use reed_solomon_erasure::galois_8::ReedSolomon;
use num_bigint::BigInt;
use lru::LruCache;
use once_cell::sync::Lazy;
use std::sync::Mutex as StdMutex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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

static PROOF_CACHE: Lazy<StdMutex<LruCache<u64, BigInt>>> = Lazy::new(|| StdMutex::new(LruCache::new(NonZero::new(10000).unwrap())));

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

        let tx_hashes: Vec<Vec<u8>> = shards[..data_shards]
            .iter()
            .map(|s| Sha256::digest(s).to_vec())
            .collect();

        let mut cache = PROOF_CACHE.lock().unwrap();
        let mut tx_primes = Vec::with_capacity(data_shards);

        for hash in &tx_hashes {
            let mut hasher = DefaultHasher::new();
            hash.hash(&mut hasher);
            let key = hasher.finish();

            if let Some(cached_prime) = cache.get(&key) {
                tx_primes.push(cached_prime.clone());
            } else {
                let prime = hash_to_prime_128(hash);
                cache.put(key, prime.clone());
                tx_primes.push(prime);
            }
        }

        let tx_accumulator = compute_accumulator_from_primes(&tx_primes);
        let tx_acc_encoded = general_purpose::STANDARD.encode(tx_accumulator.to_bytes_be().1);

        let proofs = generate_proofs_from_primes_radix(&tx_primes);
        let encoded_proofs: Vec<String> = proofs
            .iter()
            .map(|p| general_purpose::STANDARD.encode(p.to_bytes_be().1))
            .collect();

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

        let shard_hashes_hex: Vec<String> = tx_hashes.iter().map(|h| hex::encode(h)).collect();
        let padded_root = pad_to_len(content.into_bytes(), transaction_size);

        Transaction {
            root: Sha256::digest(&padded_root).to_vec(),
            shards: shard_structs,
            accumulator: Some(tx_acc_encoded),
            shard_hashes: Some(shard_hashes_hex),
        }
    }).collect();

    info!("📝 Created {} transactions in {:?}", num_txs, timer.elapsed());

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
