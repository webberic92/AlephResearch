use base64::{engine::general_purpose, Engine};
use rayon::prelude::*;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, Duration, Instant}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}};
use tracing::{error, info};
use num_bigint::{BigInt, Sign};

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{
        node::Node,
        requests::{CommitRequest, DagUnit, PrevoteRequest},
    },
    utils::rsa_accumulator_util::{get_modulus, memoized_hash_to_prime},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();

    let (node_id, round_id, quorum_threshold, data_shards, node_list, rbc_processor, transaction_size) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.data_shards,
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
            node_guard.transaction_size,
        )
    };

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voters = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        if !voters.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {} for round {}.", node_id, prevote_request.sender_url, round_id);
            return Ok(());
        }
        if voters.len() < quorum_threshold {
            info!("Node {}: Waiting for more prevotes ({}/{})", node_id, voters.len(), quorum_threshold);
            return Ok(());
        }
    }

    info!("Node {}: Quorum reached for round {}. Starting verification and reconstruction...", node_id, round_id);

    let modulus = get_modulus();
    let mut reconstructed_units = Vec::new();
    let mut total_duration_proof = Duration::ZERO;
    let mut total_duration_reconstruct = Duration::ZERO;

    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;
        let mut reconstructed_transactions = Vec::new();
        let mut duration_proof = Duration::ZERO;
        let mut duration_reconstruct = Duration::ZERO;

        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let acc_encoded = tx.accumulator.as_ref().ok_or("Missing accumulator in transaction")?;
            let acc_bytes = general_purpose::STANDARD.decode(acc_encoded)
                .map_err(|e| format!("Failed to decode accumulator: {:?}", e))?;
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

            let t1 = Instant::now();
            let mut hash_proof_pairs = Vec::new();

            for shard_index in 0..data_shards {
                let shard = &tx.shards[shard_index];

                let decoded = general_purpose::STANDARD.decode(&shard.shard_b64)
                    .map_err(|e| format!("tx[{}] shard[{}] decode error: {:?}", tx_index, shard_index, e))?;

                let expected_len = (transaction_size + data_shards - 1) / data_shards;
                if decoded.len() != expected_len {
                    return Err(format!("tx[{}] shard[{}] length mismatch", tx_index, shard_index));
                }

                let expected_hash_hex = tx.shard_hashes
                    .as_ref()
                    .and_then(|h| h.get(shard_index))
                    .ok_or_else(|| format!("Missing hash for tx[{}] shard[{}]", tx_index, shard_index))?;
                let expected_hash = hex::decode(expected_hash_hex)
                    .map_err(|e| format!("Invalid hex in shard_hash[{}]: {:?}", shard_index, e))?;

                let proof_b64 = shard.proofs.get(0)
                    .ok_or_else(|| format!("Missing proof for tx[{}] shard[{}]", tx_index, shard_index))?;
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64)
                    .map_err(|e| format!("Proof decode error: {:?}", e))?;
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

                hash_proof_pairs.push((expected_hash_hex.clone(), expected_hash, proof, decoded));
            }

            for (hash_hex, hash_bytes, proof, _) in &hash_proof_pairs {
                let prime = memoized_hash_to_prime(&node, round_id, hash_hex).await;
                let valid = proof.modpow(&prime, &modulus) == accumulator;
                if !valid {
                    return Err(format!("Node {}: tx[{}] RSA proof failed on shard {}", node_id, tx_index, hash_hex));
                }
            }

            duration_proof += t1.elapsed();

            let t2 = Instant::now();
            let padded_tx_bytes = {
                let mut buffer = Vec::with_capacity(transaction_size);
                for (_, _, _, shard_bytes) in &hash_proof_pairs {
                    buffer.extend_from_slice(shard_bytes);
                }
                buffer.truncate(transaction_size);
                buffer
            };
            duration_reconstruct += t2.elapsed();

            let hash = Sha256::digest(&padded_tx_bytes);
            if hash.to_vec() != tx.root {
                return Err(format!(
                    "Node {}: tx[{}] hash mismatch. Expected {}, got {}",
                    node_id, tx_index, hex::encode(&tx.root), hex::encode(&hash)
                ));
            }

            reconstructed_transactions.push(tx.clone());
        }

        let dag_unit = DagUnit {
            unit_id: format!("U{}-{}", round_id, proposer_id),
            proposer_node: proposer_id,
            round: round_id,
            transactions: reconstructed_transactions,
            parent_units: proposal.parents.iter().map(|p| String::from_utf8_lossy(p).to_string()).collect(),
            accumulator_root: vec![],
            finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };

        reconstructed_units.push(dag_unit);
        total_duration_proof += duration_proof;
        total_duration_reconstruct += duration_reconstruct;
    }

    info!(
        "Node {}: Round {} verified. Time: {:?} [proof: {:?}, reconstruct: {:?}]",
        node_id,
        round_id,
        timer_total.elapsed(),
        total_duration_proof,
        total_duration_reconstruct
    );

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone();
    let client = Client::builder().build().map_err(|e| format!("HTTP client error: {:?}", e))?;

    for peer in node_list {
        let url = format!("http://{}/commit", peer);
        for attempt in 0..3 {
            message_count.fetch_add(1, Ordering::Relaxed);
            match client.post(&url).json(&commit_request).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!("✅ Commit sent to {}", url);
                    break;
                }
                Ok(resp) => {
                    error!("❌ Commit failed to {}. Status: {}, Body: {}", url, resp.status(), resp.text().await.unwrap_or_default());
                }
                Err(e) => {
                    error!("❌ Commit error to {}: {:?}", url, e);
                }
            }
            sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
        }
    }

    if let Some(rbc_processor) = rbc_processor {
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request)).await;
    }

    Ok(())
}
