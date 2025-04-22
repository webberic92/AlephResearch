use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tracing::info;
use anyhow::Error;
// use num_bigint::BigInt;
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}},
    utils::rsa_accumulator_util::{compute_accumulator, generate_proof, hash_to_prime}
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
    let mut all_primes = Vec::new();
    let mut all_hashes = Vec::new(); // ✅ Needed for proof generation
    let mut shard_prime_index_map = Vec::new();
    let mut transactions = Vec::new();

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let padded = pad_to_len(content.into_bytes(), transaction_size);
        let tx_root = Sha256::digest(&padded).to_vec();

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)
            .map_err(|e| Error::msg(format!("RS init failed: {:?}", e)))?;

        // Prepare shards
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
        rs.encode(&mut shard_refs)
            .map_err(|e| Error::msg(format!("RS encoding failed: {:?}", e)))?;

        let mut tx_prime_list = Vec::new();
        let mut encoded_shards = Vec::new();

        for (shard_index, shard) in shards.iter().enumerate() {
            let hash = Sha256::digest(shard).to_vec();
            let prime = hash_to_prime(&hash);

            if shard_index < data_shards {
                all_primes.push(prime.clone());
                all_hashes.push(hash.clone()); // ✅ store original hashes
                tx_prime_list.push(prime.clone());
                encoded_shards.push(general_purpose::STANDARD.encode(shard));
            }

            // info!(
            //     "ProofGen: tx[{}] shard[{}]: SHA256 = {}, mapped_prime = {}",
            //     tx_index,
            //     shard_index,
            //     hex::encode(&hash),
            //     prime.to_str_radix(10).chars().take(12).collect::<String>()
            // );
        }

        shard_prime_index_map.push(tx_prime_list);

        transactions.push(Transaction {
            root: tx_root,
            shards: encoded_shards,
            proofs: vec![], // will be filled later
        });
    }

    // === Accumulator & Proofs ===
    let prime_input_bytes: Vec<Vec<u8>> = all_hashes.clone(); // ✅ match generation with verification
    let accumulator = compute_accumulator(&prime_input_bytes);
    let encoded_accumulator = general_purpose::STANDARD.encode(accumulator.to_bytes_be().1);

    // === Proof generation
    let flat_proofs: Vec<String> = prime_input_bytes
        .iter()
        .enumerate()
        .map(|(i, _prime_bytes_i)| {
            let proof = generate_proof(&prime_input_bytes, i, &accumulator);
            let encoded_proof = general_purpose::STANDARD.encode(proof.to_bytes_be().1);

            // let prime_bigint = BigInt::from_bytes_be(num_bigint::Sign::Plus, prime_bytes_i);

            // info!(
            //     "🧪 ProofGen: index[{}], prime = {}, proof = {}",
            //     i,
            //     prime_bigint.to_str_radix(10).chars().take(20).collect::<String>(),
            //     proof.to_str_radix(10).chars().take(20).collect::<String>(),
            // );

            encoded_proof
        })
        .collect();

    // === Assign proofs per transaction
    let mut cursor = 0;
    for (tx_index, (tx, primes_for_tx)) in transactions.iter_mut().zip(shard_prime_index_map.iter()).enumerate() {
        let end = cursor + primes_for_tx.len();
        if end > flat_proofs.len() {
            return Err(Error::msg(format!(
                "Proof slice OOB for tx[{}]: cursor={} + {} > {}",
                tx_index, cursor, primes_for_tx.len(), flat_proofs.len()
            )));
        }

        let proofs_for_tx: Vec<String> = flat_proofs[cursor..cursor + data_shards].to_vec();
        tx.proofs = proofs_for_tx;
        cursor += primes_for_tx.len();
    }

    info!(
        "✅ RSA-based proposal ready: {} txs, {} primes, Round {}, Acc: {}...",
        num_txs,
        all_primes.len(),
        round_id,
        encoded_accumulator.get(..12).unwrap_or(&encoded_accumulator)
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
