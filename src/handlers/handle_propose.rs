use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
};
use base64::{engine::general_purpose, Engine};
use tokio::{sync::Mutex, time::{sleep, Duration, Instant}};
use tracing::info;

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}},
    utils::{dag_utils::ensure_dag_round_sync, merkle_utils::verify_merkle_proof},
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;

    let node_id = {
        let guard = node.lock().await;
        guard.id
    };

    ensure_dag_round_sync(node.clone(), round_id).await?;

    {
        let guard = node.lock().await;
        let mut tracker = guard.proposal_tracker.lock().await;
        let entry = tracker.entry(round_id).or_insert_with(HashMap::new);

        if entry.contains_key(&proposer_id) {
            return Ok(());
        }

        entry.insert(proposer_id, propose_request.clone());

        let proposal_count = entry.len();
        let quorum_threshold = guard.total_nodes - guard.get_fault_tolerance_threshold();

        if proposal_count < quorum_threshold {
            return Ok(()); // ✅ Fast return before verification
        }
    }

    // ✅ Now validate Merkle proofs only after quorum
    let stored_proposals = {
        let guard = node.lock().await;
        let tracker = guard.proposal_tracker.lock().await;
        tracker.get(&round_id).unwrap().values().cloned().collect::<Vec<_>>()
    };

    for (p_idx, prop) in stored_proposals.iter().enumerate() {
        if prop.batch_proofs.len() != prop.transactions.len() {
            return Err(format!(
                "Proposal {}: proof count mismatch {} vs {}",
                p_idx, prop.batch_proofs.len(), prop.transactions.len()
            ));
        }

        for (i, tx) in prop.transactions.iter().enumerate() {
            let proof = &prop.batch_proofs[i];
            let root = &prop.batch_root;

            if tx.root.len() != 32 {
                return Err(format!("Tx {}: invalid root length {}", i, tx.root.len()));
            }

            let shard_encoded = tx.shards.first().unwrap_or(&String::new()).to_owned();
            let decoded = general_purpose::STANDARD
                .decode(&shard_encoded)
                .map_err(|e| format!("Decode failed for tx {}: {:?}", i, e))?;

            let (tx_size, data_shards) = {
                let g = node.lock().await;
                (g.transaction_size, g.data_shards)
            };
            let expected_size = (tx_size + data_shards - 1) / data_shards;

            if decoded.len() != expected_size {
                return Err(format!(
                    "Tx {}: decoded shard size mismatch (expected {}, got {})",
                    i, expected_size, decoded.len()
                ));
            }

            if !verify_merkle_proof(&tx.root, proof, root, i) {
                return Err(format!("Tx {}: invalid Merkle proof", i));
            }
        }
    }

    // ✅ Lock round proposal set after verification
    {
        let guard = node.lock().await;
        let mut locks = guard.proposal_locks.lock().await;
        if !locks.contains(&round_id) {
            locks.insert(round_id);
        }
    }

    let prevote_request = {
        let guard = node.lock().await;
        PrevoteRequest {
            proposals: stored_proposals.clone(),
            sender_url: guard.ip_address.clone(),
            sender_id: guard.id,
        }
    };

    let (node_ip, node_list) = {
        let guard = node.lock().await;
        (guard.ip_address.clone(), guard.nodes.clone())
    };

    let client = reqwest::Client::new();
    for peer in node_list {
        if peer != node_ip {
            let url = format!("http://{}/prevote", peer);
            for attempt in 1..=3 {
                if let Ok(resp) = client.post(&url).json(&prevote_request).send().await {
                    if resp.status().is_success() {
                        break;
                    }
                }
                sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
            }
            node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    if let Some(rbc_processor) = &node.lock().await.rbc_processor {
        rbc_processor.enqueue_message(RBCMessage::Prevote(prevote_request)).await;
    }

    info!(
        "Node {}: ✅ handle_propose round {} completed in {:?}",
        node_id,
        round_id,
        timer_total.elapsed()
    );

    Ok(())
}
