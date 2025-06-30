use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::{sync::Mutex, task::spawn_blocking, time::sleep};
use tracing::{error, info};

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

    {
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);

        if voter_set.len() >= quorum_threshold {
            info!("Node {}: Quorum already met for round {}", node_id, round_id);
            return Ok(());
        }

        if !voter_set.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {}", node_id, prevote_request.sender_url);
            return Ok(());
        }

        if voter_set.len() < quorum_threshold {
            info!(
                "Node {}: Waiting for quorum. Votes: {}/{}",
                node_id,
                voter_set.len(),
                quorum_threshold
            );
            return Ok(());
        }
    }

    let dag_unit_ids: HashSet<_> = node_guard
        .dag
        .lock()
        .await
        .values()
        .flatten()
        .map(|u| u.unit_id.clone())
        .collect();

    let shard_agg = node_guard.shard_aggregator.clone();
    let data_shards = node_guard.data_shards;
    let transaction_size = node_guard.transaction_size;
    let node_list = node_guard.nodes.clone();
    let rbc = node_guard.rbc_processor.clone();
    drop(node_guard); // release lock early

    let mut reconstructed_units = Vec::new();

    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;
        let batch_root = proposal.batch_root.clone();

        let results = futures::future::join_all(
            proposal.transactions.iter().enumerate().map(|(i, tx)| {
                let tx_clone = tx.clone();
                let proof_data = proposal.batch_proofs[i].clone();
                let batch_root = batch_root.clone();
                let shard_agg = shard_agg.clone();

                async move {
                    // Insert shards
                    for (j, shard_str) in tx_clone.shards.iter().enumerate() {
                        let decoded = general_purpose::STANDARD
                            .decode(shard_str)
                            .map_err(|e| format!("Decode error shard {} tx {}: {:?}", j, i, e))?;
                        if j < data_shards {
                            shard_agg.lock().await.insert_shard(round_id, i, j, decoded);
                        }
                    }

                    // Reconstruct padded tx
                    let padded_tx = match shard_agg.lock().await.try_reconstruct(round_id, i, transaction_size) {
                        Some(p) => p,
                        None => return Err(format!("Failed to reconstruct tx {}", i)),
                    };

                    // Verify hash
                    let hash = Sha256::digest(&padded_tx).to_vec();
                    if hash != tx_clone.root {
                        return Err(format!(
                            "Tx {} hash mismatch: {} vs {}",
                            i,
                            hex::encode(&tx_clone.root),
                            hex::encode(&hash)
                        ));
                    }

                    let tx_root = tx_clone.root.clone();
                    let proof_for_spawn = proof_data.clone();
                    let tx_root_for_spawn = tx_root.clone();

                    // Parallel Merkle proof validation
                    let proof_valid = spawn_blocking(move || {
                        validate_merkle_branch(&tx_root_for_spawn, &proof_for_spawn, i, &batch_root)
                    })
                    .await
                    .unwrap();

                    if !proof_valid {
                        return Err(format!("Invalid Merkle proof for tx {}", i));
                    }

                    Ok(Transaction {
                        root: tx_root,
                        proofs: vec![proof_data.iter().map(hex::encode).collect()],
                        shards: tx_clone.shards,
                    })
                }
            })
        )
        .await;

        let mut reconstructed_transactions = Vec::new();
        for result in results {
            match result {
                Ok(tx) => reconstructed_transactions.push(tx),
                Err(e) => return Err(e),
            }
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
            return Err("Node: Reconstructed unit is empty".to_string());
        }

        reconstructed_units.push(reconstructed_unit);
    }

    info!("Quorum met. Sending commits for round {}", round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units.clone(),
        proposing_node_id: node.lock().await.id,
        round_id,
    };

    if let Some(rbc) = rbc {
        rbc.enqueue_message(RBCMessage::Commit(commit_request.clone())).await;
    }

    let client = &local_client;
    let futures = node_list.iter().map(|target| {
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
            error!("Failed to send commit to {}", url);
        }
    });

    futures::future::join_all(futures).await;
    shard_agg.lock().await.clear_round(round_id);

    Ok(())
}
