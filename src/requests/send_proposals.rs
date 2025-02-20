use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    handlers::handle_propose::handle_propose,
    structs::{ node::Node, requests::{ BaseRequest, ProposeRequest } },
    utils::merkle_utils::{ compute_merkle_branch, compute_merkle_root },
};

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
    let round;
    let node_id;
    let nodes;

    {
        let node_guard = node.lock().await;
        round = propose_request.base.round_id;
        node_id = node_guard.id;
        nodes = node_guard.nodes.clone();
    }

    info!(
        "Node {}: Preparing to send a proposal with {} transactions for round {} to nodes: {:?}",
        node_id, propose_request.transactions.len(), round, nodes
    );

    // ✅ **Step 4: Iterate over all nodes `j ∈ N` and send the full proposal**
    let futures: Vec<_> = nodes.iter().map(|node_url| {
        let client = client.clone();
        let node_url = node_url.clone();
        let proposal_clone = propose_request.clone();

        async move {
            info!(
                "Node {} sending proposal to {} for round {}: {:?}",
                node_id, node_url, proposal_clone.base.round_id, proposal_clone
            );

            match client
                .post(format!("http://{}/propose", node_url))
                .json(&proposal_clone)
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

        if let Err(e) = handle_propose(node.clone(), client.clone(), propose_request).await {
            error!(
                "Node {}: Failed to handle local proposal. Error: {:?}",
                node_id, e
            );
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}


