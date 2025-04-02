use base64::{engine::general_purpose, Engine};
use futures::future::join_all;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, timeout}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}, time::Duration};
use tracing::{error, info, warn};
use reqwest::Client;
use num_bigint::BigInt;
use crate::{
    processors::priority_queue::RBCMessage, 
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, Transaction},
    }, utils::{rsa_accumulator_util::verify_proof, shard_util::{interpolate_shares, reconstruct_unit}},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,  
) -> Result<(), String> {
    let (node_id, round_id, quorum_threshold, total_nodes, node_list, rbc_processor) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.total_nodes,
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
        )
    }; 

    {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        let round_key = round_id.to_be_bytes().to_vec();

        if let Some(voter_set) = quorum_votes.get(&round_key) {
            if voter_set.len() >= quorum_threshold {
                info!(
                    "Node {}: Quorum already reached for round {} ({} votes). Dropping incoming prevote.",
                    node_guard.id, round_id, voter_set.len()
                );
                return Ok(());
            }
        }
    }

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        if voter_set.contains(&prevote_request.sender_url) {
            info!(
                "Node {}: Already received prevote from Node {} for round {}. Ignoring.",
                node_id, prevote_request.sender_url, round_id
            );
            return Ok(());
        }
    }

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let mut reconstructed_transactions = Vec::new();

        for transaction in &proposal.transactions {
            let decoded_shards: Vec<Vec<u8>> = transaction.shards
                .iter()
                .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
                .collect::<Result<Vec<Vec<u8>>, _>>()
                .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

            let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
                .map(|shard| Sha256::digest(shard).to_vec())
                .collect();

            let accumulator_bytes = general_purpose::STANDARD
                .decode(transaction.accumulator.as_bytes())
                .map_err(|e| format!("Node {}: Failed to decode accumulator: {:?}", node_id, e))?;

            let accumulator = BigInt::from_bytes_be(num_bigint::Sign::Plus, &accumulator_bytes);

            for (i, shard_hash) in shard_hashes.iter().enumerate() {
                let proof_bytes = general_purpose::STANDARD
                    .decode(transaction.proofs[0][i].as_bytes())
                    .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;

                let proof = BigInt::from_bytes_be(num_bigint::Sign::Plus, &proof_bytes);

                if !verify_proof(shard_hash, &proof, &accumulator) {
                    return Err(format!(
                        "Node {}: RSA Accumulator proof verification failed for shard {}",
                        node_id, i
                    ));
                }
            }

            let interpolated_shards = if decoded_shards.len() < total_nodes {
                interpolate_shares(&decoded_shards, round_id)
                    .map_err(|e| format!("Node {}: Failed to interpolate shares: {:?}", node_id, e))?
            } else {
                decoded_shards.clone()
            };

            let reconstructed_tx = Transaction {
                accumulator: transaction.accumulator.clone(),
                proofs: transaction.proofs.clone(),
                shards: interpolated_shards.iter().map(|s| base64::engine::general_purpose::STANDARD.encode(s)).collect(),
            };

            reconstructed_transactions.push(reconstructed_tx);
        }

        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,
            round_id,
            proposal.parents.clone(),
            proposal.base.proposing_node_id as usize,
        ).map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

        if reconstructed_unit.transactions.is_empty() {
            return Err(format!("Node {}: Reconstructed unit is invalid or empty", node_id));
        }

        reconstructed_units.push(reconstructed_unit);
    }

    let sender_id = prevote_request.sender_url;
    let round_key = round_id.to_be_bytes().to_vec();
    let vote_count;

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);
        voter_set.insert(sender_id);
        vote_count = voter_set.len();
    }

    if vote_count < quorum_threshold {
        info!(
            "Node {}: Not enough prevote messages received ({}/{}). Waiting for quorum before proceeding to commit.",
            node_id, vote_count, quorum_threshold
        );
        return Ok(());
    }

    info!("Node {}: Prevote Quorum reached for round {}. Proceeding to send commits.", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone();

    if let Some(rbc_processor) = &rbc_processor {
        info!("Node {}: Adding *local* commit to queue for round {}", node_id, round_id);
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request.clone())).await;
    } else {
        error!("Node {}: RBCProcessor not initialized when trying to enqueue *local* commit!", node_id);
    }

    for target_node in node_list {
        let target_url = format!("http://{}/commit", target_node);
        let commit_payload = commit_request.clone();
    
        let mut attempt = 0;
        let max_attempts = 3;
        let mut success = false;
    
        while attempt < max_attempts {
            attempt += 1;
    
            message_count.fetch_add(1, Ordering::Relaxed);
            info!(
                "📤 Attempt {}/{}: Node {} sending commit to {} for round {}",
                attempt, max_attempts, node_id, target_node, round_id
            );
    
            match client
                .post(&target_url)
                .json(&commit_payload)
                // .timeout(Duration::from_millis(500)) // optional
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    info!("✅ Commit successfully sent to {}", target_url);
                    success = true;
                    break;
                }
                Ok(response) => {
                    let status = response.status();
                    let msg = response.text().await.unwrap_or_else(|_| "No response".to_string());
                    error!("❌ Commit failed to {}. Status: {}. Body: {}", target_url, status, msg);
                }
                Err(e) => {
                    error!("❌ Network error while sending commit to {}: {:?}", target_url, e);
                }
            }
    
            let delay = 100 * 2u64.pow((attempt - 1) as u32); // backoff: 100ms, 200ms, 400ms
            sleep(Duration::from_millis(delay)).await;
        }
    
        if !success {
            error!("❌ Node {}: Final failure to send commit to {} after {} attempts", node_id, target_node, max_attempts);
        }
    }
    

    Ok(())
}
