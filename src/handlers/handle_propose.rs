use std::sync::Arc;
use base64::{ engine::general_purpose, Engine };
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use tracing::{ error, info };
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{ node::Node, requests::{ PrevoteRequest, ProposeRequest } },
    utils::{dag_utils::{ check_size, ensure_dag_round_sync }, merkle_utils::compute_merkle_root},
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

// pub async fn handle_propose(
//     node: Arc<Mutex<Node>>,
//     client: Arc<Client>,
//     propose_request: ProposeRequest,
// ) -> Result<(), String> {
//     let round_id = propose_request.base.round_id;
//     let node_id;

//     // 🔹 Step 1: Log receipt of the proposal
//     {
//         let node_guard = node.lock().await;
//         node_id = node_guard.id;

//         info!(
//             "============== Node {}: Handling PROPOSE request for round {} from Node {} ==============",
//             node_id, round_id, propose_request.base.proposing_node_id
//         );

//         info!(
//             "Node {}: Received PROPOSE request: {:?}",
//             node_id, propose_request
//         );

//         // 🔹 Step 2: Check if we already received a proposal from this node
//         // 🔹 Step 2: Check if we already received a proposal from this node
//         let duplicate = {
//             let proposal_tracker = node_guard.proposal_tracker.lock().await;
//             proposal_tracker.get(&round_id) // Ensure `round_id` remains `u64`
//                 .map(|round_proposals| round_proposals.contains_key(&(propose_request.base.proposing_node_id as usize))) // Convert `proposing_node_id` to `u64`
//                 .unwrap_or(false) 
//         };


//         if duplicate {
//             info!(
//                 "Node {}: Already received propose for round {} from Node {}. Terminating.",
//                 node_id, round_id, propose_request.base.proposing_node_id
//             );
//             return Ok(());
//         }
//     }

//     // 🔹 Step 3: Decode transactions' shards and validate them
//     for transaction in &propose_request.transactions {
//         let decoded_shards: Vec<Vec<u8>> = transaction.shards
//             .iter()
//             .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
//             .collect::<Result<Vec<Vec<u8>>, _>>()
//             .map_err(|e| format!("Failed to decode shards: {:?}", e))?;

//         info!("handle proposal Decoded shards: {:?}", decoded_shards);

//         let (number_of_transactions, transaction_size);
//         {
//             let node_guard = node.lock().await;
//             number_of_transactions = node_guard.number_of_transactions;
//             transaction_size = node_guard.transaction_size;
//         }

//         // 🔹 Step 3.1: Validate each transaction's shard size
//         if !check_size(&decoded_shards, number_of_transactions, transaction_size) {
//             return Err(format!(
//                 "Node {}: Received oversized unit, rejecting propose.",
//                 node_id
//             ));
//         }
//     }

//     // 🔹 Step 4: Ensure DAG is synchronized to round r - 1
//     ensure_dag_round_sync(node.clone(), round_id).await?;

//     // 🔹 Step 5: Store the proposal
//     let (proposal_count, required_proposals, stored_proposals) = Node::update_proposal_tracker(
//         node.clone(),
//         propose_request.clone(),
//     ).await?;

//     info!(
//         "Node {}: Current Proposal Tracker for round {} has {} proposals (threshold: {}).",
//         node_id, round_id, proposal_count, required_proposals
//     );

//     // 🔹 Step 6: If quorum is met, multicast aggregated prevote
//     if proposal_count >= required_proposals {
//         info!(
//             "Node {}: Proposal quorum met. Aggregating and multicasting prevote for {} proposals.",
//             node_id, stored_proposals.len()
//         );

//         // ✅ Collect all proposals individually instead of aggregating them
//         let prevote_request = {
//             let node_guard = node.lock().await;
//             PrevoteRequest {
//                 proposals: stored_proposals.clone(),  // ✅ Send as a list, not merged
//                 sender_url: node_guard.ip_address.clone(),
//             }
//         };
//         info!(
//             "Node {}: Created PrevoteRequest with data {:?}",
//             node_id, prevote_request
//         );

//         let (node_ip, node_list) = {
//             let node_guard = node.lock().await;
//             (node_guard.ip_address.clone(), node_guard.nodes.clone()) // Clone and drop lock early
//         };

//         info!("Node {}: Multicasting prevote to all nodes...", node_id);
//         for target_node in node_list {
//             if target_node != node_ip {
//                 let target_url = format!("http://{}/prevote", target_node);
//                 info!("Node {}: Sending prevote to {}", node_id, target_url);

//                 let client_clone = client.clone();
//                 let prevote_request_clone = prevote_request.clone();

//                 tokio::spawn(async move {
//                     match client_clone.post(&target_url)
//                         .json(&prevote_request_clone)
//                         .send()
//                         .await
//                     {
//                         Ok(response) if response.status().is_success() => {
//                             info!(
//                                 "Node {}: Successfully sent prevote to {}",
//                                 node_id, target_url
//                             );
//                         }
//                         Ok(response) => {
//                             error!(
//                                 "Node {}: Failed to send prevote to {}. Status: {}",
//                                 node_id, target_url, response.status()
//                             );
//                         }
//                         Err(e) => {
//                             error!(
//                                 "Node {}: Network error while sending prevote to {}: {:?}",
//                                 node_id, target_url, e
//                             );
//                         }
//                     }
//                 });
//             }
//         }

//         // 🔹 Step 7: Also handle the prevote locally
//         info!("Node {}: Handling prevote locally.", node_id);
//         handle_prevote(node.clone(), client, prevote_request).await.map_err(|e| {
//             error!(
//                 "Node {}: Failed to handle aggregated prevote for round {}. Error: {:?}",
//                 node_id, round_id, e
//             );
//             format!("Aggregated prevote phase failed: {:?}", e)
//         })?;
//     }

//     // 🔹 Step 8: Log successful handling
//     info!(
//         "============== Node {}: Proposal successfully handled for round {} from sender {} ==============",
//         node_id, round_id, propose_request.base.proposing_node_id
//     );

//     Ok(())
// }



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
    }

    for transaction in &propose_request.transactions {
        info!(
            "🔹 handle_propose: Processing transaction with received Merkle root: {:?}",
            transaction.root
        );

        // **Step 1: Decode the received shards**
        let decoded_shards: Vec<Vec<u8>> = transaction.shards
            .iter()
            .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
            .collect::<Result<Vec<Vec<u8>>, _>>()
            .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

        info!("🔹 handle_propose: Decoded shards: {:?}", decoded_shards);

        // **Step 2: Compute the SHA-256 hash of each shard**
        let shard_hashes: Vec<Vec<u8>> = decoded_shards
            .iter()
            .map(|shard| Sha256::digest(shard).to_vec())
            .collect();

        info!("🔹 handle_propose: Computed shard hashes: {:?}", shard_hashes);

        // **Step 3: Compute Merkle root from the decoded shards**
        let computed_merkle_root = compute_merkle_root(&shard_hashes);

        info!(
            "🔹 handle_propose: Computed Merkle root: {:?}, Expected Merkle root: {:?}",
            computed_merkle_root, transaction.root
        );

        // **Step 4: Validate the Merkle root**
        if computed_merkle_root != transaction.root {
            return Err(format!(
                "Node {}: Merkle root mismatch! Expected {:?}, but computed {:?}",
                node_id, transaction.root, computed_merkle_root
            ));
        }

        info!("✅ Node {}: Merkle root verification PASSED!", node_id);
    }

    info!(
        "============== Node {}: Proposal successfully handled for round {} from sender {} ==============",
        node_id, round_id, propose_request.base.proposing_node_id
    );

    Ok(())
}
