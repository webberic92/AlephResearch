
use futures::future::join_all;
use reqwest::Client;
use tracing::{error, info};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use crate::{
    handlers::handle_propose::handle_propose, processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor}, structs::{ node::Node, requests::ProposeRequest }};
/* 
**ch-RBC Proof Validation for `send_proposals`**
--------------------------------------------------

1. If `P_i = P_s`, then:
   - This function is called when a node needs to broadcast its proposal.

2. `{s_j} j∈N ← shares of (f + 1, N)-erasure coding of U`:
   - The `shards` parameter represents the erasure-coded shares.

3. `h ← Merkle tree root of {s_j} j∈N`:
   - The function `compute_merkle_root` is used to compute the Merkle root.

4. For each node `j ∈ N`:
   - A loop iterates over all nodes in `node_read.nodes` to send proposals.

5. `b_i ← Merkle branch of s_j`:
   - The function `compute_merkle_branch` computes the Merkle branch.

6. `send propose(h, b_j, s_j) to P_j`:
   - The proposals are sent asynchronously using `reqwest::Client::post`.
*/
pub async fn send_proposals(
    client: Arc<Client>,  
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), anyhow::Error> {
    info!("Node {} : Entering sending proposals for round {}", propose_request.base.proposing_node_id, propose_request.base.round_id);   
   
    // Extract values quickly and release the lock
    let (round, node_id, nodes) = {
        let node_guard = node.lock().await;
        (propose_request.base.round_id, node_guard.id, node_guard.nodes.clone())
    }; 

    info!(
        "Node {} : Preparing to send proposal to nodes {:?} for round {}",
        node_id, nodes, round
    );

    // Send proposals concurrently
    let futures: Vec<_> = nodes.iter().map(|node_url| {
        let client = client.clone();
        let node_url = node_url.clone();
        let proposal_clone = propose_request.clone();

        async move {
            info!(
                "Node {} sending proposal to {} for round {}",
                node_id, node_url, proposal_clone.base.round_id
            );

            match client
            .post(format!("http://{}/propose", node_url))
            .json(&proposal_clone)
            .timeout(Duration::from_secs(5))  // 🔥 Add a timeout!
            .send()
            .await
            {
                Ok(res) if res.status().is_success() => {
                    info!(
                        "Proposal successfully delivered to Node {} (round {}).",
                        node_url, proposal_clone.base.round_id
                    );
                    Ok(())
                }
                Ok(res) => {
                    let err_msg = format!(
                        "Proposal failed for {}. Status: {}. Response: {}",
                        node_url,
                        res.status(),
                        res.text().await.unwrap_or_else(|_| "No response body".to_string())
                    );
                    error!("{}", err_msg);
                    Err(anyhow::anyhow!(err_msg))
                }
                Err(e) => Err(anyhow::anyhow!("Network error while sending proposal to {}: {:?}", node_url, e)),
            }
        }
    }).collect();

    let results: Vec<Result<(), anyhow::Error>> = join_all(futures).await;

    if results.iter().all(|res| res.is_ok()) {
        info!("Successfully sent all proposals for round {}.", round);

        // 🔥 Run `handle_propose` in a separate **spawned task** to avoid blocking
        let node_clone = node.clone();
        let client_clone = client.clone();
        let propose_request_clone = propose_request.clone();

        let proposal_message = RBCMessage::Proposal(propose_request_clone);

        // ✅ Get `rbc_processor` from `Node`
        let node_guard = node.lock().await;
        let rbc_processor = match &node_guard.rbc_processor {
            Some(processor) => processor.clone(),
            None => {
                error!("Node {}: `rbc_processor` is not initialized!", node_id);
                return Err(anyhow::anyhow!(format!("Node {}: `rbc_processor` is not initialized!", node_id)));
            }
        };
        
        // ✅ Enqueue the proposal into the queue instead of spawning a task
        rbc_processor.enqueue_message(proposal_message).await;

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}



