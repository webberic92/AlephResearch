
use chrono::Local;
use futures::future::join_all;
use reqwest::Client;
use tracing::{error, info};
use std::{sync::{atomic::Ordering, Arc}, time::Duration};
use tokio::{sync::Mutex, time::timeout};
use crate::{
    handlers::handle_propose::handle_propose, processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor}, structs::{ node::Node, requests::ProposeRequest }};
use anyhow::anyhow;
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
    //info!("🔍 [DEBUG] Attempting to acquire lock for send_proposals() in round {}", propose_request.base.round_id);
    
    let lock_result = timeout(Duration::from_secs(5), node.lock()).await;
    
    let node_guard = match lock_result {
        Ok(guard) => {
            //info!("🔓 [DEBUG] Acquired node lock for send_proposals() in round {}", propose_request.base.round_id);
            guard
        }
        Err(_) => {
            error!("❌ [ERROR] Timeout acquiring node lock for send_proposals() in round {}. Possible deadlock!", propose_request.base.round_id);
            return Err(anyhow!("Timeout acquiring lock for send_proposals() in round {}", propose_request.base.round_id));
        }
    };

    let round_id = propose_request.base.round_id;
    let node_id = node_guard.id;
    let nodes = node_guard.nodes.clone();
    let rbc_processor = node_guard.rbc_processor.clone();  // ✅ Clone `rbc_processor` while holding the lock
    drop(node_guard); // 🔥 **Explicitly drop lock after extracting values**

    info!(
        "🚀 [DEBUG] Node {} preparing to send proposals to {:?} for round {}",
        node_id, nodes, round_id
    );


    let message_count = node.lock().await.message_count.clone(); // ✅ Clone the Arc<AtomicU64>

    let futures: Vec<_> = nodes.iter().map(|node_url| {
        let client = client.clone();
        let node_url = node_url.clone();
        let proposal_clone = propose_request.clone();
        let message_count = message_count.clone(); // ✅ Correctly cloned inside the async block

        async move {
            info!(
                "📤 Node {} sending proposal to {} for round {}",
                node_id, node_url, proposal_clone.base.round_id
            );
            message_count.fetch_add(1, Ordering::Relaxed);

            match client
                .post(format!("http://{}/propose", node_url))
                .json(&proposal_clone)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    info!(
                        "✅ Proposal successfully delivered to Node {} (round {}).",
                        node_url, proposal_clone.base.round_id
                    );
                    Ok(())
                }
                Ok(res) => {
                    let err_msg = format!(
                        "❌ Proposal failed for {}. Status: {}. Response: {}",
                        node_url,
                        res.status(),
                        res.text().await.unwrap_or_else(|_| "No response body".to_string())
                    );
                    error!("{}", err_msg);
                    Err(anyhow!(err_msg))
                }
                Err(e) => Err(anyhow!("❌ Network error while sending proposal to {}: {:?}", node_url, e)),
            }
        }
    }).collect();


    let results: Vec<Result<(), anyhow::Error>> = join_all(futures).await;

    if results.iter().all(|res| res.is_ok()) {
        info!("✅ Successfully sent all proposals for round {}.", round_id);

        // ✅ **Enqueue the proposal into the RBC queue**
        if let Some(rbc_processor) = rbc_processor {
            let proposal_message = RBCMessage::Proposal(propose_request.clone());
            info!("📥 [DEBUG] Node {} enqueuing proposal for round {} into RBCProcessor queue", node_id, round_id);
            rbc_processor.enqueue_message(proposal_message).await;
        } else {
            error!("❌ [ERROR] Node {}: RBCProcessor is not initialized when enqueuing proposal!", node_id);
            return Err(anyhow!("RBCProcessor not initialized when enqueuing proposal!"));
        }

        Ok(())
    } else {
        Err(anyhow!("❌ One or more proposals failed."))
    }
}




