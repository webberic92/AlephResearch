use std::{sync::{atomic::Ordering, Arc}, thread::sleep, time::Duration};
use base64::{ engine::general_purpose, Engine };
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{ error, info, warn };
use crate::{
    processors::priority_queue::RBCMessage, structs::{ node::Node, requests::{ PrevoteRequest, ProposeRequest } }, utils::dag_utils::{ check_size, ensure_dag_round_sync }
};


/*
**ch-RBC Proof Validation for `handle_propose`**
--------------------------------------------------

**Step 7:** Upon receiving `propose(h, b_j, s_j)` from `P_s`
   - This function (`handle_propose`) is invoked upon receiving a `ProposeRequest` from another node.

**Step 8:** If `received_propose(P_i, r)` then terminate
   - The proposal tracker is checked to ensure no duplicate proposals from the same sender (`P_s`) in the same round (`r`).
   - If a duplicate exists, the function returns early.

**Step 9:** If `check_size(s_j)` then
   - The function `check_size(&decoded_shards)` validates the shard sizes.
   - If the size is invalid, the function returns early.

**Step 20:** Wait until `D_i` reaches round `r−1`
   - The function `ensure_dag_round_sync(node.clone(), round_id).await?` ensures that the DAG is synchronized to `r-1` before proceeding.

**Step 11:** Multicast `prevote(h, b_j, s_j)` to all nodes
   - If enough proposals are received (`proposal_count >= required_proposals`), all `prevote` messages are aggregated into a single request and multicast to all nodes.

**Step 12:** `received_propose(P_i, r) = True`
   - The proposal is stored in `proposal_tracker` using `update_proposal_tracker()`, ensuring that the proposal is marked as received.

**Step 13:** Confirm proposal handling
   - Log that the proposal was successfully handled.
*/

// Step 7: Upon receiving `propose(h, b_j, s_j)` from `P_s`
// - This function `handle_propose` is called when a proposal message is received.

/*
**ch-RBC Proof Validation for `handle_propose`**
--------------------------------------------------
Handles proposals containing multiple transactions in a single request.
Each transaction has:
  - Merkle root
  - Proofs
  - Encoded shards
*/

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let round_id = propose_request.base.round_id;
    let node_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;

        // ✅ Drop early if proposal quorum already met
        let proposal_tracker = node_guard.proposal_tracker.lock().await;
        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            let required_proposals = node_guard.total_nodes - node_guard.get_fault_tolerance_threshold();
            if round_proposals.len() >= required_proposals {
                info!(
                    "Node {}: Proposal quorum already met for round {} ({} received, required_proposals: {}). Dropping.",
                    node_guard.id, round_id, round_proposals.len(), required_proposals
                );
                return Ok(());
            }
        }
    }

    // ✅ Validate shard size
    for transaction in &propose_request.transactions {
        let decoded_shards: Vec<Vec<u8>> = transaction.shards
            .iter()
            .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to decode shards: {:?}", e))?;

        let (n_tx, tx_size) = {
            let node_guard = node.lock().await;
            (node_guard.number_of_transactions, node_guard.transaction_size)
        };

        if !check_size(&decoded_shards, n_tx, tx_size) {
            return Err(format!(
                "Node {}: Received oversized unit. Rejecting proposal.",
                node_id
            ));
        }
    }

    // ✅ Ensure DAG is synchronized to r - 1
    ensure_dag_round_sync(node.clone(), round_id).await?;

    // ✅ Add proposal to tracker
    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    if proposal_count >= quorum_threshold {
        info!(
            "Node {}: Proposal quorum met. Aggregating and multicasting prevote for {} proposals.",
            node_id, stored_proposals.len()
        );

        let prevote_request = {
            let node_guard = node.lock().await;
            PrevoteRequest {
                proposals: stored_proposals.clone(),
                sender_url: node_guard.ip_address.clone(),
            }
        };

        let (node_ip, node_list) = {
            let node_guard = node.lock().await;
            (node_guard.ip_address.clone(), node_guard.nodes.clone())
        };

        info!("Node {}: Multicasting prevote to all nodes...", node_id);

        for target_node in node_list {
            if target_node != node_ip {
                let url = format!("http://{}/prevote", target_node);
                let mut attempt = 0;
                let max_attempts = 3;
                let mut success = false;
        
                while attempt < max_attempts {
                    attempt += 1;
        
                    info!(
                        "📤 Attempt {}/{}: Node {} sending prevote to {} for round {}",
                        attempt, max_attempts, node_id, url, round_id
                    );
        
                    let res = client
                        .post(&url)
                        .json(&prevote_request)
                        // .timeout(Duration::from_millis(500))  // optional
                        .send()
                        .await;
        
                    match res {
                        Ok(resp) if resp.status().is_success() => {
                            info!("✅ Node {}: Prevote success to {}", node_id, url);
                            success = true;
                            break;
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_else(|_| "No response".to_string());
                            error!("❌ Node {}: Prevote failed to {}. Status: {}, Body: {}", node_id, url, status, body);
                        }
                        Err(e) => {
                            error!("❌ Node {}: Network error sending prevote to {}: {:?}", node_id, url, e);
                        }
                    }
        
                    let delay = 100 * 2u64.pow((attempt - 1) as u32);
                    sleep(Duration::from_millis(delay));
                }
        
                if !success {
                    error!("❌ Node {}: Final failure sending prevote to {} after {} attempts", node_id, url, max_attempts);
                }
        
                node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        // ✅ Handle prevote locally
        // ✅ Queue local prevote instead of calling directly
        {
            let node_guard = node.lock().await;
            if let Some(rbc_processor) = &node_guard.rbc_processor {
                info!("Node {}: Enqueuing local prevote into RBCProcessor for round {}", node_id, round_id);
                rbc_processor
                    .enqueue_message(RBCMessage::Prevote(prevote_request))
                    .await;
            } else {
                error!("Node {}: RBCProcessor not initialized for local prevote enqueue!", node_id);
            }
        }
    }

    Ok(())
}



