// Copy this file as-is into your handle_prevote.rs

use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, timeout}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}, time::Duration};
use tracing::{error, info, warn};
use reqwest::Client;
use crate::{
    processors::priority_queue::RBCMessage, 
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, Transaction},
    }, 
    utils::merkle_utils::{reconstruct_unit, validate_merkle_branch}
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,  
) -> Result<(), String> {
    let (node_id, round_id, quorum_threshold, total_nodes, data_shards, node_list, rbc_processor, transaction_size) = {
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

    {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        let round_key = round_id.to_be_bytes().to_vec();
        if let Some(voter_set) = quorum_votes.get(&round_key) {
            if voter_set.len() >= quorum_threshold {
                info!("Node {}: Quorum already reached for round {} ({} votes). Dropping incoming prevote.", node_guard.id, round_id, voter_set.len());
                return Ok(());
            }
        }
    }

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        if voter_set.contains(&prevote_request.sender_url) {
            info!("Node {}: Already received prevote from Node {} for round {}. Ignoring.", node_id, prevote_request.sender_url, round_id);
            return Ok(());
        }
    } 

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let mut reconstructed_transactions = Vec::new();
        let batch_root = &proposal.batch_root;
        let batch_proofs = &proposal.batch_proofs;

        let proposer_id = proposal.base.proposing_node_id as usize;
        for (i, transaction) in proposal.transactions.iter().enumerate() {
            for (j, shard_str) in transaction.shards.iter().enumerate() {
                
                match general_purpose::STANDARD.decode(shard_str) {
                    Ok(decoded) => {
                        let (transaction_size, data_shards) = {
                            let node_guard = node.lock().await;
                            (node_guard.transaction_size, node_guard.data_shards)
                        };
                        let expected_len = (transaction_size + data_shards - 1) / data_shards;                        if decoded.len() != expected_len {
                            return Err(format!(
                                "Node {}: Shard {} for tx {} is not {} bytes (got {})",
                                node_id, j, i, expected_len, decoded.len()
                            ));
                        }
                        let node_guard = node.lock().await;
                        let mut shard_aggregator = node_guard.shard_aggregator.lock().await;
                        if j < data_shards {
                            // shard_aggregator.insert_shard(round_id, i, j, decoded.clone());
                            // shard_aggregator.insert_shard(round_id, i, proposer_id, decoded.clone());
                            // shard_aggregator.insert_shard(round_id, i, prevote_request.sender_id, decoded.clone());
                            shard_aggregator.insert_shard(round_id, i, j, decoded.clone());
                            info!(
                                "Node {}: Inserting shard j={} for tx[{}] from proposer {} into aggregator (round {})",
                                node_id, j, i, proposer_id, round_id
                            );
                        }               
                                 // shard_aggregator.insert_shard(round_id, i, proposer_id, decoded.clone());
                    }
                    Err(e) => {
                        warn!("Node {}: Failed to decode shard {} for tx {}: {:?}", node_id, j, i, e);
                    }
                }
            }

            let maybe_reconstructed = {
                let node_guard = node.lock().await;
                let shard_aggregator = node_guard.shard_aggregator.lock().await;
                
                shard_aggregator.try_reconstruct(round_id, i, transaction_size)
            };

            let padded_tx_bytes = match maybe_reconstructed {
                Some(data) => data,
                None => {
                    warn!("Node {}: Not enough shards to reconstruct tx[{}] in round {}. Waiting for more...", node_id, i, round_id);
                    return Ok(());
                }
            };
            info!("Node {}: Reconstructed tx[{}] with {} bytes for round {}", node_id, i, padded_tx_bytes.len(), round_id); 
            info!(
                "Node {}: TX[{}] padded bytes = {:?}",
                node_id, i, padded_tx_bytes
            );
            let hash = Sha256::digest(&padded_tx_bytes).to_vec();
            info!(
                "Node {}: TX[{}] hash = {} (expected: {})",
                node_id,
                i,
                hex::encode(&hash),
                hex::encode(&transaction.root)
            );
            
            if hash != transaction.root {
                return Err(format!(
                    "Node {}: Hash mismatch for tx {}. Expected {}, got {}",
                    node_id,
                    i,
                    hex::encode(&transaction.root),
                    hex::encode(&hash)
                ));
            }

            let proof = &batch_proofs[i];
            if !validate_merkle_branch(&transaction.root, proof, i, batch_root) {
                return Err(format!(
                    "Node {}: Invalid Merkle proof for transaction {} in batch",
                    node_id, i
                ));
            }

            reconstructed_transactions.push(Transaction {
                root: transaction.root.clone(),
                proofs: vec![batch_proofs[i].iter().map(hex::encode).collect()],
                shards: transaction.shards.clone(),
            });
        }


        let mut resolved_parents = Vec::new();
        let node_guard = node.lock().await;
        let dag_guard = node_guard.dag.lock().await;
        
        for hash in &proposal.parents {
            let hash_hex = hex::encode(hash);
            let maybe_match = dag_guard.values().flatten().find_map(|unit| {
                if Sha256::digest(unit.unit_id.as_bytes()).to_vec() == *hash {
                    Some(unit.unit_id.clone())
                } else {
                    None
                }
            });
        
            if let Some(unit_id) = maybe_match {
                resolved_parents.push(unit_id);
            } else {
                warn!("Could not resolve parent hash {} to a known unit_id", hash_hex);
            }
        }




        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,
            round_id,
            resolved_parents,
            proposer_id,
            batch_root.clone(),
        )?;

        if reconstructed_unit.transactions.is_empty() {
            return Err(format!("Node {}: Reconstructed unit is invalid or empty", node_id));
        }

        reconstructed_units.push(reconstructed_unit);
    }

    {
        let node_guard = node.lock().await;
        for unit in &reconstructed_units {
            for parent_unit_id in &unit.parent_units {
                if !node_guard.is_unit_committed(&parent_unit_id).await {
                    let err_msg = format!("Node {}: Missing parent unit {} in DAG. Cannot commit unit in round {}.", node_guard.id, parent_unit_id, round_id);
                    error!("{}", err_msg);
                    return Err(err_msg);
                }
            }
        }
    }

    let sender_id = prevote_request.sender_url;
    let vote_count;
    let round_key = round_id.to_be_bytes().to_vec();

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);
        voter_set.insert(sender_id);
        vote_count = voter_set.len();
    }

    if vote_count < quorum_threshold {
        info!("Node {}: Not enough prevote messages received ({}/{}). Waiting for quorum before proceeding to commit.", node_id, vote_count, quorum_threshold);
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
            info!("\u{1f4e4} Attempt {}/{}: Node {} sending commit to {} for round {}", attempt, max_attempts, node_id, target_url, round_id);

            let res = client.post(&target_url).json(&commit_payload).send().await;

            match res {
                Ok(response) if response.status().is_success() => {
                    info!("\u{2705} Successfully sent commit to {}", target_url);
                    success = true;
                    break;
                }
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_else(|_| "No response".to_string());
                    error!("\u{274c} Commit failed to {}. Status: {}. Body: {}", target_url, status, body);
                }
                Err(e) => {
                    error!("\u{274c} Network error while sending commit to {}: {:?}", target_url, e);
                }
            }

            let delay = 100 * 2u64.pow((attempt - 1) as u32);
            sleep(Duration::from_millis(delay)).await;
        }

        if !success {
            error!("\u{274c} Node {}: Final failure to send commit to {} after {} attempts", node_id, target_url, max_attempts);
        }
    }

    {
        let node_guard = node.lock().await;
        let mut shard_aggregator = node_guard.shard_aggregator.lock().await;
        shard_aggregator.clear_round(round_id);
    }

    Ok(())
}
