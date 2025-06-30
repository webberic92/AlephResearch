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

    let node_guard = node.lock().await;
    node_guard.message_count.fetch_add(1, Ordering::Relaxed);

    let node_id = node_guard.id;
    let round_id = prevote_request.proposals[0].base.round_id;
    let quorum_threshold = node_guard.get_quorum_threshold();
    let round_key = round_id.to_be_bytes().to_vec();

    // Step 1: Vote tracking
    {
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes
            .entry(round_key.clone())
            .or_insert_with(HashSet::new);

        if voter_set.len() >= quorum_threshold {
            info!("Node {}: Quorum already met for round {}", node_id, round_id);
            return Ok(());
        }

        if !voter_set.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {}", node_id, prevote_request.sender_url);
            return Ok(());
        }

        // Only proceed with commit if this vote pushed us over the threshold
        if voter_set.len() < quorum_threshold {
            info!("Node {}: Waiting for quorum. Votes: {}/{}", node_id, voter_set.len(), quorum_threshold);
            return Ok(());
        }
    }

    // Step 2: DAG cache (for resolved parents)
    let dag_unit_ids: HashSet<_> = node_guard
        .dag
        .lock()
        .await
        .values()
        .flatten()
        .map(|u| u.unit_id.clone())
        .collect();

    // Step 3: Transaction reconstruction only AFTER quorum
    let mut reconstructed_units = Vec::new();
    let mut shard_agg = node_guard.shard_aggregator.lock().await;

    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;
        let mut reconstructed_transactions = Vec::new();

        for (i, tx) in proposal.transactions.iter().enumerate() {
            for (j, shard_str) in tx.shards.iter().enumerate() {
                let decoded = general_purpose::STANDARD
                    .decode(shard_str)
                    .map_err(|e| format!("Node {}: Decode error shard {} tx {}: {:?}", node_id, j, i, e))?;
                if j < node_guard.data_shards {
                    shard_agg.insert_shard(round_id, i, j, decoded);
                }
            }

            let padded_tx = match shard_agg.try_reconstruct(round_id, i, node_guard.transaction_size) {
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
                    node_id, i, hex::encode(&tx.root), hex::encode(&hash)
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

        let resolved_parents = proposal
            .parents
            .iter()
            .filter(|pid| dag_unit_ids.contains(*pid))
            .cloned()
            .collect::<Vec<_>>();

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

    // Step 4: Commit multicast
    info!("Node {}: Quorum met. Sending commits for round {}", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units.clone(),
        proposing_node_id: node_id,
        round_id,
    };

    if let Some(rbc) = &node_guard.rbc_processor {
        rbc.enqueue_message(RBCMessage::Commit(commit_request.clone())).await;
    }

    let client = &local_client;
    let futures = node_guard.nodes.iter().map(|target| {
        let url = format!("http://{}/commit", target);
        let commit_request = commit_request.clone();
        let client = client.clone();
        let node = node.clone();
        async move {
            for attempt in 1..=3 {
                node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
                match tokio::time::timeout(
                    Duration::from_secs(3),
                    client.post(&url).json(&commit_request).send(),
                )
                .await
                {
                    Ok(Ok(resp)) if resp.status().is_success() => {
                        info!("✅ Commit sent to {}", url);
                        return;
                    }
                    Ok(Ok(resp)) => error!("❌ Commit to {} failed. Status: {}", url, resp.status()),
                    Ok(Err(e)) => error!("❌ Commit error to {}: {:?}", url, e),
                    Err(_) => error!("⏱️ Commit to {} timed out", url),
                }
                sleep(Duration::from_millis(100 * 2u64.pow(attempt - 1))).await;
            }
            error!("Node {}: Failed to send commit to {}", node_id, url);
        }
    });

    futures::future::join_all(futures).await;
    shard_agg.clear_round(round_id);

    Ok(())
}
