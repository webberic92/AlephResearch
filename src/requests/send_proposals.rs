use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use reqwest::Client;
use sha2::Digest;
use tracing::{error, info};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{
    handlers::handle_prevote::handle_prevote,
    requests::ip_server_requests::notify_transaction_submitted,
    structs::{ node::Node, requests::{ BaseRequest, PrevoteRequest, ProposeRequest } },
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
    client: &Client,
    node: Arc<RwLock<Node>>,
    shards: &[Vec<u8>],
    merkle_root: &[u8]
) -> Result<(), anyhow::Error> {
    
    let node_read = node.read().await;
    let epoch = *node_read.current_epoch.lock().await;

    info!(
        "Node {} {}: Preparing to send proposals for epoch {} to nodes: {:?}",
        node_read.id,
        node_read.ip_address,
        epoch,
        node_read.nodes
    );

    // Step 3: Compute Merkle root
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    let computed_root = compute_merkle_root(&shard_hashes);
    if computed_root != merkle_root {
        return Err(
            anyhow::anyhow!(
                "Computed Merkle root does not match provided root.\nComputed: {:?}\nProvided: {:?}",
                computed_root,
                merkle_root
            )
        );
    }

    // Step 2: Encode shards for transmission
    let encoded_shards: Vec<String> = shards
        .iter()
        .map(|shard| general_purpose::STANDARD.encode(shard))
        .collect();

    // Step 5: Compute Merkle branches for each shard
    let encoded_proofs: Vec<Vec<String>> = shard_hashes
        .iter()
        .enumerate()
        .map(|(i, _)| compute_merkle_branch(&shard_hashes, i))
        .map(|branch|
            branch
                .iter()
                .map(|b| general_purpose::STANDARD.encode(b))
                .collect()
        )
        .collect();

    let base_request = BaseRequest {
        proposing_node_id: node_read.id,
        epoch_id: epoch,
        root: merkle_root.to_vec(),
    };

    let propose_request = ProposeRequest {
        base: base_request,
        proofs: encoded_proofs.clone(),
        shards: encoded_shards.clone(),
    };

    drop(node_read); // 🔥 Release lock before async calls

    // Step 6: Send `propose(h, b_j, s_j)` to all nodes
    let futures: Vec<_> = node
        .read().await
        .nodes.iter()
        .map(|node_url| {
            let client = client.clone();
            let propose_request = propose_request.clone();
            let node_url = node_url.clone();

            async move {
                info!(
                    "Node sending proposal to {} for epoch {}",
                    node_url,
                    propose_request.base.epoch_id
                );

                match
                    client
                        .post(format!("http://{}/propose", node_url))
                        .json(&propose_request)
                        .send().await
                {
                    Ok(res) if res.status().is_success() => {
                        info!(
                            "Proposal successfully delivered to Node {} (Epoch {}).",
                            node_url,
                            propose_request.base.epoch_id
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
                    Err(e) => {
                        let err_msg = format!(
                            "Network error while sending proposal to {}: {:?}",
                            node_url,
                            e
                        );
                        error!("{}", err_msg);
                        Err(anyhow::anyhow!(err_msg))
                    }
                }
            }
        })
        .collect();

    let results = join_all(futures).await;

    // Step 1: If all proposals are sent, add self to proposal tracker
    if results.iter().all(|res| res.is_ok()) {
        info!("Successfully sent all proposals for epoch {}.", epoch);

        match Node::update_proposal_tracker(node.clone(), propose_request.clone()).await {
            Ok((proposal_count, required_proposals, stored_proposals)) => {
                info!(
                    "Node {}: Added itself to proposal tracker. Proposal count: {} / Required: {}",
                    propose_request.base.proposing_node_id,
                    proposal_count,
                    required_proposals
                );

                if proposal_count >= required_proposals {
                    info!(
                        "Node {}: Received enough proposals ({}/{}) for epoch {}. Transitioning to prevote.",
                        propose_request.base.proposing_node_id,
                        proposal_count,
                        required_proposals,
                        propose_request.base.epoch_id
                    );

                    // Send prevotes for all stored proposals
                    for stored_propose in stored_proposals {
                        info!(
                            "Node {}: Sending prevote for transaction proposed by Node {}",
                            propose_request.base.proposing_node_id,
                            stored_propose.base.proposing_node_id
                        );

                        let prevote_request = {
                            let node_read = node.read().await;
                            PrevoteRequest {
                                propose: stored_propose.clone(),
                                sender_url: node_read.ip_address.clone(),
                            }
                        };

                        if let Err(e) = handle_prevote(
                            node.clone(),
                            client.clone().into(),
                            prevote_request
                        ).await {
                            error!(
                                "Node {}: Failed to handle prevote for epoch {}. Error: {:?}",
                                propose_request.base.proposing_node_id,
                                stored_propose.base.epoch_id,
                                e
                            );
                        }
                    }

                    let node_write = node.write().await;
                    let mut proposal_tracker = node_write.proposal_tracker.lock().await;
                    proposal_tracker.clear();
                }
            }
            Err(err) => {
                error!(
                    "Node {}: Failed to update proposal tracker: {}",
                    propose_request.base.proposing_node_id,
                    err
                );
            }
        }

        if let Err(err) = notify_transaction_submitted(client, node.clone()).await {
            error!("Failed to notify transaction submitted: {:?}", err);
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}
