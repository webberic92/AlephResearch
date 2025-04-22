use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, Duration}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}};
use tracing::{error, info, warn};
use reqwest::Client;
use num_bigint::BigInt;
use crate::{
    processors::priority_queue::RBCMessage, 
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest},
    }, 
    utils::rsa_accumulator_util::verify_proof,
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let (node_id, round_id, quorum_threshold, _total_nodes, data_shards, node_list, rbc_processor, transaction_size) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.total_nodes,
            node_guard.data_shards,
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
            node_guard.transaction_size,
        )
    };

    // Drop if quorum already reached
    {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        let round_key = round_id.to_be_bytes().to_vec();
        if let Some(voter_set) = quorum_votes.get(&round_key) {
            if voter_set.len() >= quorum_threshold {
                info!("Node {}: Quorum already reached for round {} ({} votes). Dropping prevote.", node_id, round_id, voter_set.len());
                return Ok(());
            }
        }
    }

    // Drop duplicate votes
    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        if !voter_set.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Already received prevote from {} for round {}.", node_id, prevote_request.sender_url, round_id);
            return Ok(());
        }
    }

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let accumulator_bytes = general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .map_err(|e| format!("Node {}: Failed to decode accumulator: {:?}", node_id, e))?;
        let accumulator = BigInt::from_bytes_be(num_bigint::Sign::Plus, &accumulator_bytes);

        let proposer_id = proposal.base.proposing_node_id as usize;
        let mut reconstructed_transactions = Vec::new();

        for (i, tx) in proposal.transactions.iter().enumerate() {
            for (j, shard_str) in tx.shards.iter().enumerate() {
                let decoded = general_purpose::STANDARD
                    .decode(shard_str)
                    .map_err(|e| format!("Node {}: Failed to decode shard tx[{}] shard[{}]: {:?}", node_id, i, j, e))?;

                let expected_len = (transaction_size + data_shards - 1) / data_shards;
                if decoded.len() != expected_len {
                    return Err(format!(
                        "Node {}: Shard size mismatch for tx[{}] shard[{}]: expected {}, got {}",
                        node_id, i, j, expected_len, decoded.len()
                    ));
                }

                let hash = Sha256::digest(&decoded);
                // let prime = hash_to_prime(&decoded);
                // info!(
                //     "Node {}: tx[{}] shard[{}]: decoded len={}, sha256={}, mapped_prime={}",
                //     node_id,
                //     i,
                //     j,
                //     decoded.len(),
                //     hex::encode(&hash),
                //     prime.to_str_radix(10).chars().take(12).collect::<String>() // just preview
                // );
                


                if j < data_shards {
                    let node_guard = node.lock().await;
                    let mut aggregator = node_guard.shard_aggregator.lock().await;
                    aggregator.insert_shard(round_id, i, j, decoded.clone());
                }

                // ✅ Verify RSA proof per shard
                let proof_str = tx.proofs.get(j).ok_or_else(|| format!("Node {}: Missing proof for tx[{}] shard[{}]", node_id, i, j))?;
                let proof_bytes = general_purpose::STANDARD
                    .decode(proof_str)
                    .map_err(|e| format!("Node {}: Failed to decode proof for tx[{}] shard[{}]: {:?}", node_id, i, j, e))?;
                let proof = BigInt::from_bytes_be(num_bigint::Sign::Plus, &proof_bytes);

                if !verify_proof(&accumulator, &hash, &proof) {
                    return Err(format!("Node {}: Invalid RSA proof for tx[{}] shard[{}]", node_id, i, j));
                }
            }

            
            // ✅ Reconstruct padded tx
            let maybe_tx = {
                let node_guard = node.lock().await;
                let aggregator = node_guard.shard_aggregator.lock().await;
                aggregator.try_reconstruct(round_id, i, transaction_size)
            };

            let padded_tx_bytes = match maybe_tx {
                Some(data) => data,
                None => {
                    warn!("Node {}: Not enough shards yet for tx[{}] round {}. Waiting...", node_id, i, round_id);
                    return Ok(());
                }
            };

            // ✅ Verify transaction hash
            let hash = Sha256::digest(&padded_tx_bytes);
            if hash.to_vec() != tx.root {
                return Err(format!(
                    "Node {}: Hash mismatch for tx[{}]. Expected {}, got {}",
                    node_id, i, hex::encode(&tx.root), hex::encode(&hash)
                ));
            }

            reconstructed_transactions.push(tx.clone());
        }

        // ✅ Directly build unit — no Merkle
        let unit_id = format!("U{}-{}", round_id, proposer_id);
        let unit = crate::structs::requests::DagUnit {
            unit_id,
            proposer_node: proposer_id,
            round: round_id,
            transactions: reconstructed_transactions,
            parent_units: proposal.parents.iter().map(|p| String::from_utf8_lossy(p).to_string()).collect(),
            accumulator_root: accumulator_bytes.clone(), // repurpose field for accumulator root if needed
            finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };

        reconstructed_units.push(unit);
    }

    // Validate parents
    {
        let node_guard = node.lock().await;
        for unit in &reconstructed_units {
            for parent in &unit.parent_units {
                if !node_guard.is_unit_committed(parent).await {
                    return Err(format!("Node {}: Missing parent {} for round {}", node_id, parent, round_id));
                }
            }
        }
    }

    // Vote recording done earlier, now check quorum
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

    // Send commit to others
    let message_count = node.lock().await.message_count.clone();
    for peer in node_list {
        let url = format!("http://{}/commit", peer);
        let payload = commit_request.clone();
        let mut attempt = 0;

        while attempt < 3 {
            attempt += 1;
            message_count.fetch_add(1, Ordering::Relaxed);
            match client.post(&url).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!("✅ Commit sent to {}", url);
                    break;
                }
                Ok(resp) => {
                    let code = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    error!("❌ Commit failed to {}. Status: {}, Body: {}", url, code, body);
                }
                Err(e) => {
                    error!("❌ Commit error to {}: {:?}", url, e);
                }
            }
            sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
        }
    }

    // Enqueue local commit
    if let Some(rbc_processor) = &rbc_processor {
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request)).await;
    }

    // Clear aggregator state
    {
        let node_guard = node.lock().await;
        let mut aggregator = node_guard.shard_aggregator.lock().await;
        aggregator.clear_round(round_id);
    }

    Ok(())
}

