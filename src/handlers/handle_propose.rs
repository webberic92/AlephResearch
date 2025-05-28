use std::sync::{atomic::Ordering, Arc};
use base64::{engine::general_purpose, Engine};
use num_bigint::{BigInt, Sign};
use reqwest::Client;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use tracing::{error, info};
use rayon::prelude::*;
use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}},
    utils::{dag_utils::ensure_dag_round_sync, rsa_accumulator_util::{hash_to_prime_128, get_modulus}},
};

// Minimal relay logic to support quorum, then broadcast prevote
// Decoding/verification deferred until handle_prevote

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let mut duration_dag_sync = Duration::ZERO;
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;
    let node_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        let proposal_tracker = node_guard.proposal_tracker.lock().await;
        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            if round_proposals.contains_key(&proposer_id) {
                info!("Node {}: Duplicate proposal {} for round {}", node_id, proposer_id, round_id);
                return Ok(());
            }
        }
    }

    ensure_dag_round_sync(node.clone(), round_id).await?;
    duration_dag_sync = timer_total.elapsed();

    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    if proposal_count < quorum_threshold {
        info!("Node {}: Waiting for quorum ({} < {})", node_id, proposal_count, quorum_threshold);
        return Ok(());
    }

    info!("Node {}: Quorum reached for round {}, broadcasting prevote...", node_id, round_id);

    // Verify each transaction proof using precomputed shard_hashes
    if let Some(acc_encoded) = propose_request.transactions.first().and_then(|tx| tx.accumulator.clone()) {
        let acc_bytes = base64::engine::general_purpose::STANDARD.decode(acc_encoded)
            .map_err(|e| format!("Accumulator base64 decode error: {:?}", e))?;
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

        for tx in &propose_request.transactions {
            let Some(shard_hashes) = &tx.shard_hashes else {
                return Err("Missing shard_hashes field for transaction".to_string());
            };

            let proof_checks: Result<Vec<_>, _> = tx.shards.par_iter().enumerate().map(|(j, proof_list)| {
                if j >= shard_hashes.len() || proof_list.proofs.is_empty() {
                    return Ok(None);
                }

                let proof_b64 = &proof_list.proofs[0];
                let proof_bytes = base64::engine::general_purpose::STANDARD.decode(proof_b64)
                    .map_err(|e| format!("Proof decode error: {:?}", e))?;
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

                let hash_bytes = hex::decode(&shard_hashes[j])
                    .map_err(|e| format!("Hash decode error: {:?}", e))?;
                let prime = hash_to_prime_128(&hash_bytes);

                let valid = proof.modpow(&prime, &get_modulus()) == accumulator;
                if !valid {
                    return Err(format!("RSA proof verification failed for tx shard {}", j));
                }

                Ok(Some(()))
            }).collect();

            proof_checks?;
        }
    }

    let prevote_request = {
        let node_guard = node.lock().await;
        PrevoteRequest {
            proposals: stored_proposals.clone(),
            sender_url: node_guard.ip_address.clone(),
            sender_id: node_guard.id,
        }
    };

    let (node_ip, node_list) = {
        let node_guard = node.lock().await;
        (node_guard.ip_address.clone(), node_guard.nodes.clone())
    };

    let client = Client::builder().build().map_err(|e| format!("HTTP client build error: {:?}", e))?;
    for peer in node_list {
        if peer != node_ip {
            let url = format!("http://{}/prevote", peer);
            for attempt in 1..=3 {
                if let Ok(resp) = client.post(&url).json(&prevote_request).send().await {
                    if resp.status().is_success() { break; }
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
        "Node {}: handle_propose round {} done in {:?} [dag_sync: {:?}]",
        node_id, round_id, timer_total.elapsed(), duration_dag_sync
    );
    Ok(())
}
