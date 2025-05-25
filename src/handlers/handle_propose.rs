use std::sync::{atomic::Ordering, Arc};
use base64::{engine::general_purpose, Engine};
use num_bigint::{BigInt, Sign};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use tracing::{error, info};

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}},
    utils::{dag_utils::ensure_dag_round_sync, rsa_accumulator_util::{hash_to_prime_128, get_modulus}},
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let mut duration_verify = Duration::ZERO;
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
                info!(
                    "Node {}: Duplicate proposal from proposer {} for round {}. Ignoring.",
                    node_id, proposer_id, round_id
                );
                return Ok(());
            }
        }
    }

    info!("🔍 Node {}: Received proposal from proposer {} for round {}", node_id, proposer_id, round_id);

    let acc_bytes = general_purpose::STANDARD
        .decode(&propose_request.batch_accumulator)
        .map_err(|e| format!("Node {}: Failed to decode batch accumulator: {:?}", node_id, e))?;

    let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
    let modulus = get_modulus();

    let t1 = Instant::now();
    let mut batch_hashes = Vec::new();
    let mut batch_proofs = Vec::new();

    for (i, tx) in propose_request.transactions.iter().enumerate() {
        let shard = tx.shards.get(0)
            .ok_or_else(|| format!("Node {}: Missing shard for tx {}", node_id, i))?;

        let proof_b64 = shard.proofs.get(0)
            .ok_or_else(|| format!("Node {}: Missing proof for shard[0] of tx {}", node_id, i))?;

        let proof_bytes = general_purpose::STANDARD
            .decode(proof_b64)
            .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;

        let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

        let expected_hash_hex = tx.shard_hashes.as_ref()
            .and_then(|h| h.get(0))
            .ok_or_else(|| format!("Node {}: Missing shard hash for tx[{}] shard[0]", node_id, i))?;

        let hash_bytes = hex::decode(expected_hash_hex)
            .map_err(|e| format!("Node {}: Invalid hex hash for tx[{}] shard[0]: {:?}", node_id, i, e))?;

        batch_hashes.push(hash_bytes);
        batch_proofs.push(proof);
    }

    let batch_valid = batch_hashes.iter().zip(batch_proofs.iter()).all(|(hash, proof)| {
        let prime = hash_to_prime_128(hash);
        proof.modpow(&prime, &modulus) == accumulator
    });

    if !batch_valid {
        return Err(format!("❌ Node {}: RSA batch proof verification failed.", node_id));
    }
    duration_verify = t1.elapsed();

    let t2 = Instant::now();
    ensure_dag_round_sync(node.clone(), round_id).await?;
    duration_dag_sync = t2.elapsed();

    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    if proposal_count >= quorum_threshold {
        info!(
            "Node {}: Proposal quorum met. Broadcasting prevote for {} proposals.",
            node_id, stored_proposals.len()
        );

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

        let local_client = Client::builder()
            .pool_max_idle_per_host(64)
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .build()
            .expect("Failed to build HTTP client");

        for target_node in node_list {
            if target_node != node_ip {
                let url = format!("http://{}/prevote", target_node);
                for attempt in 1..=3 {
                    let res = local_client.post(&url).json(&prevote_request).send().await;

                    match res {
                        Ok(resp) if resp.status().is_success() => {
                            info!("✅ Node {}: Prevote delivered to {}", node_id, url);
                            break;
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_default();
                            error!("❌ Node {}: Prevote failed to {}: {} - {}", node_id, url, status, body);
                        }
                        Err(e) => {
                            error!("❌ Node {}: Network error to {}: {:?}", node_id, url, e);
                        }
                    }

                    sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
                }

                node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        if let Some(rbc_processor) = &node.lock().await.rbc_processor {
            rbc_processor
                .enqueue_message(RBCMessage::Prevote(prevote_request))
                .await;
        } else {
            error!("Node {}: No RBCProcessor to enqueue local prevote", node_id);
        }
    }

    info!(
        "Node {}: handle_propose round {} done in {:?} [verify: {:?}, dag_sync: {:?}]",
        node_id, round_id, timer_total.elapsed(), duration_verify, duration_dag_sync
    );

    Ok(())
}
