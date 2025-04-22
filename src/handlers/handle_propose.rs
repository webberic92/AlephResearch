use std::sync::{atomic::Ordering, Arc};
use base64::{engine::general_purpose, Engine};
use num_bigint::BigInt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}},
    utils::{
        dag_utils::ensure_dag_round_sync,
        rsa_accumulator_util::{hash_to_prime, verify_proof}
    },
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
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

    // Decode accumulator
    let accumulator_bytes = general_purpose::STANDARD
        .decode(&propose_request.batch_accumulator)
        .map_err(|e| format!("Node {}: Failed to decode accumulator: {:?}", node_id, e))?;
    let accumulator = BigInt::from_bytes_be(num_bigint::Sign::Plus, &accumulator_bytes);

    // Validate each tx in the proposal
    for (i, tx) in propose_request.transactions.iter().enumerate() {
        // Step 1: Decode first shard
        let shard_b64 = tx.shards.get(0)
            .ok_or_else(|| format!("Node {}: Missing shard for tx {}", node_id, i))?;
        let decoded_shard = general_purpose::STANDARD
            .decode(shard_b64)
            .map_err(|e| format!("Node {}: Failed to decode shard for tx {}: {:?}", node_id, i, e))?;

        // Step 2: Decode proof
        let proof_b64 = tx.proofs.get(0)
            .ok_or_else(|| format!("Node {}: Missing proof for tx {}", node_id, i))?;
        let proof_bytes = general_purpose::STANDARD
            .decode(proof_b64)
            .map_err(|e| format!("Node {}: Failed to decode proof for tx {}: {:?}", node_id, i, e))?;
        let proof = BigInt::from_bytes_be(num_bigint::Sign::Plus, &proof_bytes);

        // Step 3: Hash the decoded shard
        let hash = Sha256::digest(&decoded_shard);
        let hash_hex = hex::encode(&hash);
        let prime = hash_to_prime(&hash);

        // Step 4: Log all triplet info
        info!(
            "🧪 handle_propose(): tx[{}] shard[0] hash={}, prime={}, proof_b64={}",
            i,
            &hash_hex[..8.min(hash_hex.len())],
            prime.to_str_radix(10).chars().take(12).collect::<String>(),
            &proof_b64[..10.min(proof_b64.len())]
        );

        // Step 5: Verify RSA accumulator proof
        if !verify_proof(&accumulator, &hash, &proof) {
            return Err(format!(
                "❌ Node {}: RSA proof INVALID for tx {} (shard 0)",
                node_id, i
            ));
        } else {
            info!("✅ Node {}: RSA proof verified for tx[{}] shard[0]", node_id, i);
        }
    }

    // Wait for DAG to sync
    ensure_dag_round_sync(node.clone(), round_id).await?;

    // Update proposal tracker
    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    // If quorum reached, broadcast prevote
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

        for target_node in node_list {
            if target_node != node_ip {
                let url = format!("http://{}/prevote", target_node);
                for attempt in 1..=3 {
                    info!("📤 Attempt {}/3: Node {} → {}", attempt, node_id, url);
                    let res = client.post(&url).json(&prevote_request).send().await;

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
                        Err(e) => error!("❌ Node {}: Network error to {}: {:?}", node_id, url, e),
                    }

                    sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
                }
                node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        if let Some(rbc_processor) = &node.lock().await.rbc_processor {
            info!("Node {}: Enqueuing local prevote", node_id);
            rbc_processor
                .enqueue_message(RBCMessage::Prevote(prevote_request))
                .await;
        } else {
            error!("Node {}: No RBCProcessor to enqueue local prevote", node_id);
        }
    }

    Ok(())
}
