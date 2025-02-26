use std::sync::Arc;
use base64::{ engine::general_purpose, Engine };
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{ error, info };
use crate::{
    handlers::handle_prevote::handle_prevote, processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor}, structs::{ node::Node, requests::{ PrevoteRequest, ProposeRequest } }, utils::{dag_utils::{ check_size, ensure_dag_round_sync }, merkle_utils::compute_merkle_root}
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

**Step 10:** Wait until `D_i` reaches round `r−1`
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
    
    // ✅ Step 1: Acquire the node ID and log
    {
        info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
        node_id = node_guard.id;
        info!(
            "============== Node {}: Handling PROPOSE request for round {} from Node {} ==============",
            node_id, round_id, propose_request.base.proposing_node_id
        );
    }

    // ✅ Step 2: Check if proposal was already processed
    {
        info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
        let proposal_tracker = node_guard.proposal_tracker.lock().await;

        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            if round_proposals.contains_key(&(propose_request.base.proposing_node_id as usize)) {
                info!(
                    "Node {}: Already received propose for round {} from Node {}. Ignoring duplicate.",
                    node_id, round_id, propose_request.base.proposing_node_id
                );
                return Ok(()); // ✅ Exit early
            }
        }
    }

    // ✅ Step 3: Decode and validate transaction shards
    for transaction in &propose_request.transactions {
        let decoded_shards: Vec<Vec<u8>> = transaction.shards
            .iter()
            .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
            .collect::<Result<Vec<Vec<u8>>, _>>()
            .map_err(|e| format!("Failed to decode shards: {:?}", e))?;

        let (number_of_transactions, transaction_size) = {
            info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
            (node_guard.number_of_transactions, node_guard.transaction_size)
        };

        if !check_size(&decoded_shards, number_of_transactions, transaction_size) {
            return Err(format!("Node {}: Received oversized unit, rejecting propose.", node_id));
        }
    }

    // ✅ Step 4: Ensure DAG synchronization before processing the proposal
    ensure_dag_round_sync(node.clone(), round_id).await?;

    // ✅ Step 5: Store the proposal
    let (proposal_count, required_proposals, stored_proposals) = Node::update_proposal_tracker(
        node.clone(),
        propose_request.clone(),
    ).await?;

    info!(
        "Node {}: Proposal Tracker for round {}: {}/{} proposals.",
        node_id, round_id, proposal_count, required_proposals
    );

    // ✅ Step 6: If quorum is met, multicast aggregated prevote
    if proposal_count >= required_proposals {
        info!(
            "Node {}: Proposal quorum met. Aggregating and multicasting prevote for {} proposals.",
            node_id, stored_proposals.len()
        );

        // ✅ Collect all proposals instead of aggregating them
        let prevote_request = {
            info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
            PrevoteRequest {
                proposals: stored_proposals.clone(),
                sender_url: node_guard.ip_address.clone(),
            }
        };

        // ✅ Extract `node_list` before dropping the lock
        let node_list = {
            info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
            node_guard.nodes.clone() // ✅ Clone the list before unlocking
        };

        // ✅ Step 7: Send prevote requests in parallel (network is I/O bound)
        info!("Node {}: Multicasting prevote to all nodes...", node_id);
        let client_clone = client.clone();

        let prevote_futures: Vec<_> = node_list.into_iter().map(|target_node| {
            let target_url = format!("http://{}/prevote", target_node);
            let client = client_clone.clone();
            let prevote_request = prevote_request.clone();

            async move {
                match client.post(&target_url).json(&prevote_request).send().await {
                    Ok(response) if response.status().is_success() => {
                        info!("✅ Node {}: Sent prevote to {} for round {}", node_id, target_url, round_id);
                    }
                    Ok(response) => {
                        error!("❌ Node {}: Failed to send prevote to {}. Status: {}", node_id, target_url, response.status());
                    }
                    Err(e) => {
                        error!("❌ Node {}: Network error while sending prevote to {}: {:?}", node_id, target_url, e);
                    }
                }
            }
        }).collect();

        // ✅ Run all requests concurrently
        futures::future::join_all(prevote_futures).await;

        // ✅ Step 8: Handle prevote locally using `rbc_processor`
        info!("Node {}: Enqueuing prevote locally for round {}.", node_id, round_id);
        let prevote_message = RBCMessage::Prevote(prevote_request);

        // ✅ Extract `rbc_processor` before unlocking
        let rbc_processor = {
            info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
            node_guard.rbc_processor.clone()
        };

        if let Some(rbc_processor) = rbc_processor {
            rbc_processor.enqueue_message(prevote_message).await;
        } else {
            error!("❌ Node {}: RBCProcessor not initialized when enqueuing prevote!", node_id);
        }
    }

    // ✅ Step 9: Log successful handling
    info!(
        "============== Node {}: Proposal successfully handled for round {} from sender {} ==============",
        node_id, round_id, propose_request.base.proposing_node_id
    );

    Ok(())
}


