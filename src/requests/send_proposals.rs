use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    handlers::handle_prevote::handle_prevote,
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
    client: Arc<Client>,  // ✅ Changed to Arc<Client>
    node: Arc<Mutex<Node>>,
    shards: &[Vec<u8>],
    merkle_root: &[u8],
    parent_units: Vec<String>,
) -> Result<(), anyhow::Error> {
    let round;
    let node_id;
    let nodes;

    {
        let node_guard = node.lock().await;
        round = *node_guard.current_round.lock().await;
        node_id = node_guard.id;
        nodes = node_guard.nodes.clone();
    }

    info!(
        "Node {}: Preparing to send proposals for round {} to nodes: {:?}",
        node_id, round, nodes
    );

    // Step 3: Compute Merkle root
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    let computed_root = compute_merkle_root(&shard_hashes);
    if computed_root != merkle_root {
        return Err(anyhow::anyhow!(
            "Computed Merkle root does not match provided root.\nComputed: {:?}\nProvided: {:?}",
            computed_root, merkle_root
        ));
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
            branch.iter()
                .map(|b| general_purpose::STANDARD.encode(b))
                .collect()
        )
        .collect();

    let base_request = BaseRequest {
        proposing_node_id: node_id,
        round_id: round,
        root: merkle_root.to_vec(),
    };

    let propose_request = ProposeRequest {
        base: base_request,
        proofs: encoded_proofs.clone(),
        shards: encoded_shards.clone(),
        parents: parent_units.clone(),
    };

    // Step 6: Send `propose(h, b_j, s_j)` to all nodes
    let futures: Vec<_> = nodes.iter().map(|node_url| {
        let propose_request = propose_request.clone();
        let node_url = node_url.clone();
        let client = client.clone(); // ✅ Clone client for each async call

        async move {
            info!(
                "Node sending proposal to {} for round {}",
                node_url,
                propose_request.base.round_id
            );

            match client
                .post(format!("http://{}/propose", node_url))
                .json(&propose_request)
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    info!(
                        "Proposal successfully delivered to Node {} (round {}).",
                        node_url,
                        propose_request.base.round_id
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
    }).collect();

    let results = join_all(futures).await;

    // Step 7: If all proposals are sent, add self to proposal tracker
    if results.iter().all(|res| res.is_ok()) {
        info!("Successfully sent all proposals for round {}.", round);

        let update_result = {
            let node_clone = node.clone();
            Node::update_proposal_tracker(node_clone, propose_request.clone()).await
        };

        match update_result {
            Ok((proposal_count, required_proposals, stored_proposals)) => {
                info!(
                    "Node {}: Proposal tracker updated. Proposals: {}/{}",
                    propose_request.base.proposing_node_id,
                    proposal_count,
                    required_proposals
                );

                if proposal_count >= required_proposals {
                    info!(
                        "Node {}: Reached proposal threshold. Initiating prevote...",
                        propose_request.base.proposing_node_id
                    );

                    // Send prevotes for all stored proposals
                    for stored_propose in stored_proposals {
                        let prevote_request = PrevoteRequest {
                            propose: stored_propose.clone(),
                            sender_url: {
                                let node_guard = node.lock().await;
                                node_guard.ip_address.clone()
                            },
                        };

                        if let Err(e) = handle_prevote(node.clone(), client.clone(), prevote_request).await {
                            error!(
                                "Node {}: Failed to handle prevote. Error: {:?}",
                                propose_request.base.proposing_node_id,
                                e
                            );
                        }
                    }

                    // Clear proposal tracker after processing
                    {
                        let node_guard = node.lock().await;
                        node_guard.proposal_tracker.lock().await.clear();
                    }
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

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}
