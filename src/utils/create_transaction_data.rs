use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::info;
use anyhow::Error;
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}},
    utils::rsa_accumulator_util::{compute_accumulator, generate_proofs, hash_to_prime}
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
    let mut all_hashes = Vec::new();
    let mut shard_prime_index_map = Vec::new();
    let mut transactions = Vec::new();

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

        let mut tx_prime_list = Vec::new();
        let mut encoded_shards = Vec::new();

        for (shard_index, shard) in shards.iter().enumerate() {
            let hash = Sha256::digest(shard).to_vec();
            let prime = hash_to_prime(&hash);

            if shard_index < data_shards {
                all_hashes.push(hash.clone());
                tx_prime_list.push(prime.clone());
                encoded_shards.push(general_purpose::STANDARD.encode(shard));
            }

            info!(
                "🧬 ProofGen: tx[{}] shard[{}]: hash={}, prime={}, global_index={}",
                tx_index,
                shard_index,
                hex::encode(&hash)[..8.min(hash.len())].to_string(),
                prime.to_str_radix(10).chars().take(12).collect::<String>(),
                all_hashes.len()
            );
        }

        transactions.push(Transaction {
            root: tx_root,
            shards: encoded_shards,
            proofs: vec![],
        });

        shard_prime_index_map.push(tx_prime_list);
    }

    let accumulator = compute_accumulator(&all_hashes);
    let encoded_accumulator = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    let now = std::time::Instant::now();
    let raw_proofs = generate_proofs(&all_hashes);
    let mut flat_proofs: Vec<String> = raw_proofs
        .into_iter()
        .map(|proof| general_purpose::STANDARD.encode(proof.to_bytes_be().1))
        .collect();
    info!("⏱️ Proof generation done in {:?}", now.elapsed());

    for (tx_index, (tx, primes_for_tx)) in transactions.iter_mut().zip(shard_prime_index_map.iter()).enumerate() {
        if flat_proofs.len() < primes_for_tx.len() {
            return Err(Error::msg(format!(
                "💥 Not enough proofs: tx[{}] expects {} proofs, but only {} left",
                tx_index, primes_for_tx.len(), flat_proofs.len()
            )));
        }

        let proofs_for_tx: Vec<String> = flat_proofs.drain(..primes_for_tx.len()).collect();
        tx.proofs = proofs_for_tx;

        info!(
            "🧩 Proofs assigned to tx[{}]: {:?}",
            tx_index,
            tx.proofs.iter().map(|p| p.chars().take(10).collect::<String>()).collect::<Vec<_>>()
        );
    }

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
