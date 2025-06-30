use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::{sync::Mutex, time::sleep};
use tracing::{error, info, warn};
use crate::{
    processors::priority_queue::RBCMessage,
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, Transaction},
    },
    utils::merkle_utils::{reconstruct_unit, validate_merkle_branch},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let local_client = reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .build()
        .expect("Failed to build HTTP client");

    // Extract static values early
    let (node_id, round_id, quorum_threshold, node_list, rbc_processor, transaction_size, data_shards) = {
        let guard = node.lock().await;
        guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            guard.id,
            prevote_request.proposals[0].base.round_id,
            guard.get_quorum_threshold(),
            guard.nodes.clone(),
            guard.rbc_processor.clone(),
            guard.transaction_size,
            guard.data_shards,
        )
    };
    let round_key = round_id.to_be_bytes().to_vec();

    // Check quorum already reached
    {
        let node_guard = node.lock().await;
        let quorum_votes_guard = node_guard.quorum_votes.lock().await;
        if let Some(voter_set) = quorum_votes_guard.get(&round_key) {
            if voter_set.len() >= quorum_threshold {
                info!("Node {}: Quorum already met for round {}", node_id, round_id);
                return Ok(());
            }
        }
    }

    // Insert vote
    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);
        if !voter_set.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {}", node_id, prevote_request.sender_url);
            return Ok(());
        }
    }

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;
        let mut reconstructed_transactions = Vec::new();

        for (i, tx) in proposal.transactions.iter().enumerate() {
            for (j, shard_str) in tx.shards.iter().enumerate() {
                let decoded = general_purpose::STANDARD.decode(shard_str).map_err(|e| {
                    format!("Node {}: Decode error shard {} tx {}: {:?}", node_id, j, i, e)
                })?;

                let expected_len = (transaction_size + data_shards - 1) / data_shards;
                if decoded.len() != expected_len {
                    return Err(format!(
                        "Node {}: Shard {} of tx {} expected {} bytes, got {}",
                        node_id, j, i, expected_len, decoded.len()
                    ));
                }

                if j < data_shards {
                    let node_guard = node.lock().await;
                    let mut aggregator = node_guard.shard_aggregator.lock().await;
                    aggregator.insert_shard(round_id, i, j, decoded);
                }
            }

            // Try reconstruct
            let padded_tx_opt = {
                let node_guard = node.lock().await;
                let aggregator = node_guard.shard_aggregator.lock().await;
                aggregator.try_reconstruct(round_id, i, transaction_size)
            };

            let padded_tx = match padded_tx_opt {
                Some(p) => p,
                None => {
                    warn!("Node {}: Cannot reconstruct tx {} in round {}", node_id, i, round_id);
                    return Ok(());
                }
            };

            let hash = Sha256::digest(&padded_tx).to_vec();
            if hash != tx.root {
                return Err(format!(
                    "Node {}: Tx {} hash mismatch: {} vs {}",
                    node_id,
                    i,
                    hex::encode(&tx.root),
                    hex::encode(&hash)
                ));
            }

            let proof = &proposal.batch_proofs[i];
            if !validate_merkle_branch(&tx.root, proof, i, &proposal.batch_root) {
                return Err(format!(
                    "Node {}: Invalid Merkle proof for tx {} in round {}",
                    node_id, i, round_id
                ));
            }

            reconstructed_transactions.push(Transaction {
                root: tx.root.clone(),
                proofs: vec![proof.iter().map(hex::encode).collect()],
                shards: tx.shards.clone(),
            });
        }

        // Check parents exist
        let resolved_parents = {
            let node_guard = node.lock().await;
            let dag_guard = node_guard.dag.lock().await;
            proposal
                .parents
                .iter()
                .filter(|pid| dag_guard.values().flatten().any(|u| &u.unit_id == *pid))
                .cloned()
                .collect::<Vec<_>>()
        };

        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,
            round_id,
            resolved_parents,
            proposer_id,
            proposal.batch_root.clone(),
        )?;

        if reconstructed_unit.transactions.is_empty() {
            return Err(format!("Node {}: Reconstructed unit is empty", node_id));
        }

        reconstructed_units.push(reconstructed_unit);
    }

    // Check parent commitments
    {
        let node_guard = node.lock().await;
        for unit in &reconstructed_units {
            for parent in &unit.parent_units {
                if !node_guard.is_unit_committed(parent).await {
                    return Err(format!("Node {}: Missing committed parent {}", node_id, parent));
                }
            }
        }
    }

    // Check updated vote count
    let vote_count = {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        quorum_votes[&round_key].len()
    };

    if vote_count < quorum_threshold {
        info!("Node {}: Waiting for quorum. Current votes: {}/{}", node_id, vote_count, quorum_threshold);
        return Ok(());
    }

    info!("Node {}: Quorum reached. Broadcasting commits for round {}", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units.clone(),
        proposing_node_id: node_id,
        round_id,
    };

    if let Some(rbc) = &rbc_processor {
        rbc.enqueue_message(RBCMessage::Commit(commit_request.clone())).await;
    }

    for target in node_list {
        let url = format!("http://{}/commit", target);
        let mut success = false;

        for attempt in 1..=3 {
            node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
            let res = tokio::time::timeout(
                Duration::from_secs(3),
                local_client.post(&url).json(&commit_request).send(),
            )
            .await;

            match res {
                Ok(Ok(resp)) if resp.status().is_success() => {
                    info!("✅ Commit sent to {}", url);
                    success = true;
                    break;
                }
                Ok(Ok(resp)) => error!("❌ Commit to {} failed. Status: {}", url, resp.status()),
                Ok(Err(e)) => error!("❌ Commit error to {}: {:?}", url, e),
                Err(_) => error!("⏱️ Commit to {} timed out", url),
            }

            sleep(Duration::from_millis(100 * 2u64.pow(attempt - 1))).await;
        }

        if !success {
            error!("Node {}: Failed to send commit to {}", node_id, url);
        }
    }

    {
        let node_guard = node.lock().await;
        node_guard.shard_aggregator.lock().await.clear_round(round_id);
    }

    Ok(())
}
