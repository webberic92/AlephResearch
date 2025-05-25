use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, Duration}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}};
use tracing::{error, info, warn};
use reqwest::Client;
use num_bigint::{BigInt, Sign};
use rayon::prelude::*;

use crate::{
    processors::priority_queue::RBCMessage, 
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest},
    }, 
    utils::rsa_accumulator_util::{verify_proof, hash_to_prime_128, get_modulus},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
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
        let quorum_votes = node_guard.quorum_votes.lock().await;
        if let Some(voter_set) = quorum_votes.get(&round_id.to_be_bytes().to_vec()) {
            if voter_set.len() >= quorum_threshold {
                info!("Node {}: Quorum already reached for round {}. Dropping prevote.", node_id, round_id);
                return Ok(());
            }
        }
    }

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let entry = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        if !entry.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {} for round {}.", node_id, prevote_request.sender_url, round_id);
            return Ok(());
        }
    }

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;

        // ✅ decode batch accumulator ONCE
        let acc_bytes = base64::engine::general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .map_err(|e| format!("Node {}: Failed to decode batch accumulator: {:?}", node_id, e))?;
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let modulus = get_modulus();

        let mut reconstructed_transactions = Vec::new();

        for (i, tx) in proposal.transactions.iter().enumerate() {
            let verified_shards: Result<Vec<_>, String> = tx.shards.par_iter().enumerate()
                .filter(|(j, _)| *j < data_shards)
                .map(|(j, shard_struct)| {
                    let decoded = general_purpose::STANDARD
                        .decode(&shard_struct.shard_b64)
                        .map_err(|e| format!("tx[{}] shard[{}] decode error: {:?}", i, j, e))?;

                    let expected_len = (transaction_size + data_shards - 1) / data_shards;
                    if decoded.len() != expected_len {
                        return Err(format!("tx[{}] shard[{}]: expected {}, got {}", i, j, expected_len, decoded.len()));
                    }

                    let hash = Sha256::digest(&decoded);
                    let proof_str = shard_struct.proofs.get(0)
                        .ok_or_else(|| format!("tx[{}] shard[{}]: missing proof", i, j))?;
                    let proof_bytes = general_purpose::STANDARD.decode(proof_str)
                        .map_err(|e| format!("tx[{}] shard[{}] proof decode error: {:?}", i, j, e))?;
                    let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                    let prime = hash_to_prime_128(&hash);

                    if proof.modpow(&prime, &modulus) != accumulator {
                        return Err(format!("tx[{}] shard[{}]: RSA proof invalid", i, j));
                    }

                    Ok((j, decoded))
                }).collect();

            let verified = verified_shards?;

            // ✅ Insert all verified shards in one lock
            {
                let node_guard = node.lock().await;
                let mut aggregator = node_guard.shard_aggregator.lock().await;
                for (j, decoded) in verified {
                    aggregator.insert_shard(round_id, i, j, decoded);
                }
            }

            // ✅ Attempt reconstruction
            let padded_tx_bytes = {
                let node_guard = node.lock().await;
                let aggregator = node_guard.shard_aggregator.lock().await;
                match aggregator.try_reconstruct(round_id, i, transaction_size) {
                    Some(data) => data,
                    None => {
                        warn!("Node {}: Not enough shards for tx[{}] in round {}.", node_id, i, round_id);
                        return Ok(());
                    }
                }
            };

            let hash = Sha256::digest(&padded_tx_bytes);
            if hash.to_vec() != tx.root {
                return Err(format!(
                    "Node {}: tx[{}] hash mismatch. Expected {}, got {}",
                    node_id, i, hex::encode(&tx.root), hex::encode(&hash)
                ));
            }

            reconstructed_transactions.push(tx.clone());
        }

        reconstructed_units.push(crate::structs::requests::DagUnit {
            unit_id: format!("U{}-{}", round_id, proposer_id),
            proposer_node: proposer_id,
            round: round_id,
            transactions: reconstructed_transactions,
            parent_units: proposal.parents.iter().map(|p| String::from_utf8_lossy(p).to_string()).collect(),
            accumulator_root: vec![],
            finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    {
        let node_guard = node.lock().await;
        for unit in &reconstructed_units {
            for parent in &unit.parent_units {
                if !node_guard.is_unit_committed(parent).await {
                    warn!("Node {}: Missing parent {} for round {}", node_id, parent, round_id);
                }
            }
        }
    }

    let vote_count = {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        quorum_votes.get(&round_id.to_be_bytes().to_vec()).unwrap().len()
    };

    if vote_count < quorum_threshold {
        info!("Node {}: Waiting for more prevotes ({}/{})", node_id, vote_count, quorum_threshold);
        return Ok(());
    }

    info!("Node {}: Quorum reached for round {}. Broadcasting commits...", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone();
    for peer in node_list {
        let url = format!("http://{}/commit", peer);
        let payload = commit_request.clone();
        let local_client = Client::builder()
            .pool_max_idle_per_host(64)
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .build()
            .expect("Failed to build HTTP client");

        for attempt in 0..3 {
            message_count.fetch_add(1, Ordering::Relaxed);
            match local_client.post(&url).json(&payload).send().await {
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

    {
        let node_guard = node.lock().await;
        let mut aggregator = node_guard.shard_aggregator.lock().await;
        aggregator.clear_round(round_id);
    }

    Ok(())
}

