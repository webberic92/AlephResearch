use std::sync::Arc;
use base64::{ engine::general_purpose, Engine };
use reqwest::Client;
use tokio::sync::{Mutex, RwLock};
use tracing::{ error, info };
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{ node::Node, requests::{ PrevoteRequest, ProposeRequest } },
    utils::dag_utils::{ check_size, ensure_dag_round_sync },
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
pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let round_id = propose_request.base.round_id;
    let node_id;

    // 🔹 Step 7: Log receipt of the proposal
    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;

        info!(
            "============== Node {}: Handling PROPOSE request for round {} from Node {}==============",
            node_id, round_id, propose_request.base.proposing_node_id
        );

        // 🔹 Step 8: Check if we already received a proposal from this node
        let duplicate = {
            let proposal_tracker = node_guard.proposal_tracker.lock().await;
            proposal_tracker.get(&round_id)
                .map(|round_proposals| round_proposals.contains_key(&propose_request.base.proposing_node_id))
                .unwrap_or(false) 
        }; // 🔥 Lock is dropped here
        
        if duplicate {
            info!(
                "Node {}: Already received propose for round {} from Node {}. Terminating.",
                node_id, round_id, propose_request.base.proposing_node_id
            );
            return Ok(()); // ✅ Step 8 fulfilled
        }
        
    }

    // 🔹 Step 9: Decode shards and validate their size
    let decoded_shards: Vec<Vec<u8>> = propose_request.shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Failed to decode shards: {:?}", e))?;

    let (number_of_transactions, transaction_size);
    {
        let node_guard = node.lock().await;
        number_of_transactions = node_guard.number_of_transactions;
        transaction_size = node_guard.transaction_size;
    }

    // 🔹 Step 9 continued: Validate shard size
    if !check_size(&decoded_shards, number_of_transactions, transaction_size) {
        return Err(format!("Node {}: Received oversized unit, rejecting propose.", node_id));
    }

    // 🔹 Step 10: Wait until DAG is synchronized to round r - 1
    ensure_dag_round_sync(node.clone(), round_id).await?;

    // 🔹 Step 12: Add proposal to tracker, marking as received
    let (proposal_count, required_proposals, stored_proposals) = Node::update_proposal_tracker(
        node.clone(),
        propose_request.clone(),
    ).await?;

    info!(
        "Node {}: Current Proposal Tracker for round {} has {} proposals (threshold: {}).",
        node_id, round_id, proposal_count, required_proposals
    );

    // 🔹 Step 11: If quorum is met, multicast aggregated prevote
    if proposal_count >= required_proposals {
        info!(
            "Node {}: Proposal quorum met. Aggregating and multicasting prevote for {} proposals.",
            node_id, stored_proposals.len()
        );

        let mut aggregated_shards = Vec::new();
        let mut aggregated_proofs = Vec::new();
        let mut aggregated_parents = Vec::new();
        info!("Node {}:Attempting to Aggregate prevote data...", node_id);
        // 🔹 Aggregate all proposals
        for proposal in stored_proposals {
            aggregated_shards.extend(proposal.shards.clone());
            aggregated_proofs.extend(proposal.proofs.clone());
            aggregated_parents.extend(proposal.parents.clone());
        }
        info!("Node {}:Finsihed Aggregate prevote data...", node_id);

        // 🔹 Prepare the `PrevoteRequest`
        let prevote_request = {
            let node_guard = node.lock().await;
            PrevoteRequest {
                propose: ProposeRequest {
                    base: propose_request.base.clone(),
                    shards: aggregated_shards.clone(),
                    proofs: aggregated_proofs.clone(),
                    parents: aggregated_parents.clone(),
                },
                sender_url: node_guard.ip_address.clone(),
            }
        };
        info!("Node {}:Created PrevoteRequest with aggregated data", node_id);

        let (node_ip, node_list) = {
            let node_guard = node.lock().await;
            (node_guard.ip_address.clone(), node_guard.nodes.clone()) // Clone and drop lock early
        };
        
        info!("Node {}: Multicasting prevote to all nodes...", node_id);
        for target_node in node_list {  // Now we don't hold the lock
            if target_node != node_ip {
                let target_url = format!("http://{}/prevote", target_node);
                info!("Node {}: Sending prevote to {}", node_id, target_url);
        
                let client_clone = client.clone();
                let prevote_request_clone = prevote_request.clone();
                
                tokio::spawn(async move {
                    match client_clone.post(&target_url)
                        .json(&prevote_request_clone)
                        .send()
                        .await
                    {
                        Ok(response) if response.status().is_success() => {
                            info!("Node {}: Successfully sent prevote to {}", node_id, target_url);
                        }
                        Ok(response) => {
                            error!("Node {}: Failed to send prevote to {}. Status: {}", node_id, target_url, response.status());
                        }
                        Err(e) => {
                            error!("Node {}: Network error while sending prevote to {}: {:?}", node_id, target_url, e);
                        }
                    }
                });
            }
        }
        

        // 🔹 Step 11 (continued): Also handle the prevote locally
        info!("Node {}: Handling prevote locally.", node_id);
        handle_prevote(node.clone(), client, prevote_request).await.map_err(|e| {
            error!(
                "Node {}: Failed to handle aggregated prevote for round {}. Error: {:?}",
                node_id, round_id, e
            );
            format!("Aggregated prevote phase failed: {:?}", e)
        })?;
    }

    // 🔹 Step 13: Log successful handling
    info!(
        "============== Node {}: Proposal successfully handled for round {} from sender {}==============",
        node_id, round_id, propose_request.base.proposing_node_id
    );

    Ok(())
}


