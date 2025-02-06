use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use reqwest::Client;
use sha2::Digest;
use tracing::{ error, info };
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{
    handlers::handle_prevote::handle_prevote,
    requests::ip_server_requests::notify_transaction_submitted,
    structs::{ node::Node, requests::{ BaseRequest, PrevoteRequest, ProposeRequest } },
    utils::merkle_utils::{ compute_merkle_branch, compute_merkle_root },
};

/// Sends proposal messages to all nodes in the network.
pub async fn send_proposals(
    client: &Client,
    node: Arc<RwLock<Node>>,
    shards: &[Vec<u8>],
    merkle_root: &[u8]
) -> Result<(), anyhow::Error> {
    // ✅ Use anyhow::Error

    let node_read = node.read().await;

    let epoch = *node_read.current_epoch.lock().await;
    info!(
        "Node {} {}: Preparing to send proposals for epoch {} to nodes: {:?}",
        node_read.id,
        node_read.ip_address,
        epoch,
        node_read.nodes
    );

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

    let encoded_shards: Vec<String> = shards
        .iter()
        .map(|shard| general_purpose::STANDARD.encode(shard))
        .collect();

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

    if results.iter().all(|res| res.is_ok()) {
        info!("Successfully sent all proposals for epoch {}.", epoch);

        // 🔥 **Add itself to proposal tracker since its proposal was successfully sent**
        match Node::update_proposal_tracker(node.clone(), propose_request.clone()).await {
            Ok((proposal_count, required_proposals, stored_proposals)) => {
                info!(
                    "Node {}: Added itself to proposal tracker. Proposal count: {} / Required: {} : Stored proposals: {:?}",
                    propose_request.base.proposing_node_id,
                    proposal_count,
                    required_proposals,
                    stored_proposals
                );

                if proposal_count >= required_proposals {
                    info!(
                        "Node {}: Received enough proposals ({}/{}) for epoch {}. Transitioning to prevote.",
                        propose_request.base.proposing_node_id,
                        proposal_count,
                        required_proposals,
                        propose_request.base.epoch_id
                    );

                    // ✅ Send prevotes for all stored proposals
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

                        if
                            let Err(e) = handle_prevote(
                                node.clone(),
                                client.clone().into(),
                                prevote_request
                            ).await
                        {
                            error!(
                                "Node {}: Failed to handle prevote for epoch {}. Error: {:?}",
                                propose_request.base.proposing_node_id,
                                stored_propose.base.epoch_id,
                                e
                            );
                        }
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

        if let Err(err) = notify_transaction_submitted(client, node.clone()).await {
            error!("Failed to notify transaction submitted: {:?}", err);
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}
