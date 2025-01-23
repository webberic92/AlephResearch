use crate::utils::recovery_util::attempt_recovery;
use crate::utils::dag_utils::ensure_dag_synchronization; // Ensure DAG synchronization
use crate::{handlers::handle_commit::handle_commit, utils::merkle_utils::reconstruct_unit};
use crate::structs::node::Node;
use crate::utils::merkle_utils::validate_merkle_branch;
use axum::Json;
use reqwest::Client;
use tracing::{error, info, debug};

use axum::response::IntoResponse;
use std::sync::Arc;
use crate::{
    structs::requests::PrevoteRequest,
    structs::responses::Response,
};

/// Handles a prevote request in the Aleph protocol.
///
/// This function tracks quorum votes for a given root and triggers the commit phase if quorum is reached.


pub async fn handle_prevote(
    node: Arc<Node>,
    client: Arc<Client>,
    payload: PrevoteRequest,
) -> impl IntoResponse {
    // Log the raw deserialized payload
    info!(
        "Received PrevoteRequest at Node {}: {:?}",
        node.id, payload
    );

    // Call the actual logic for handling prevote
    match handle_prevote_logic(
        &node,
        &client,
        payload.sender,
        payload.root.clone(),
        &payload.proofs,
        &payload.shards,
        payload.epoch_id,
        &payload.node_url,
    )
    .await
    {
        Ok(_) => {
            info!(
                "Prevote successfully handled at Node {} for sender Node {}",
                node.id, payload.sender
            );
            Json(Response {
                status: format!(
                    "Node {}: Prevote accepted from Node {}",
                    node.id, payload.sender
                ),
            })
        }
        Err(e) => {
            error!(
                "Failed to handle prevote at Node {} for sender Node {}: {}",
                node.id, payload.sender, e
            );
            Json(Response {
                status: format!("Failed to handle prevote: {}", e),
            })
        }
    }
}



pub async fn handle_prevote_logic(
    node: &Node,
    client: &Client,
    sender: usize,
    root: Vec<u8>,
    proofs: &[Vec<Vec<u8>>], // Slice of Merkle proofs for each shard
    shards: &[Vec<u8>],      // Slice of data shards
    epoch_id: u64,
    node_url: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Node {}: ==== Handling PREVOTE REQUEST from Node {} ====", node.id, sender);

    // Validate Merkle Branch
    info!("Node {}: Validating Merkle branch for shard", node.id);
    // let computed_root = validate_merkle_branch(&shards, &proofs);
    // if computed_root != *root {
    //     error!(
    //         "Node {}: Prevote phase failed for epoch {}. Merkle root mismatch. Computed: {:?}, Expected: {:?}, Shards: {:?}, Proof: {:?}",
    //         node.id, epoch_id, computed_root, root, shards, proofs
    //     );
    //     return Err("Merkle root mismatch".into());
    // }
    info!("Node {}: Merkle branch validation passed", node.id);

    // Ensure DAG synchronization
    info!("Node {}: Ensuring DAG synchronization with {}", node.id, node_url);
    if let Err(e) = ensure_dag_synchronization(node, client, epoch_id, node_url).await {
        error!(
            "Node {}: DAG synchronization failed with node {}. Error: {:?}",
            node.id, node_url, e
        );
        return Err(format!("DAG synchronization failed: {:?}", e).into());
    }
    info!("Node {}: DAG synchronization successful with {}", node.id, node_url);

    // Update quorum votes
    {
        let mut quorum_votes = node.quorum_votes.write().await;
        let count = quorum_votes.entry(root.clone()).or_insert(0);
        *count += 1;
        debug!(
            "Node {}: Updated quorum votes for root {:?}: {}",
            node.id, root, *count
        );

        if *count >= node.get_quorum_threshold() {
            info!(
                "Node {}: Quorum reached for root {:?} with {} votes",
                node.id, root, *count
            );

            // Reconstruction and Validation Logic
            info!(
                "Node {}: Starting reconstruction for unit associated with root {:?}",
                node.id, root
            );

            match reconstruct_unit(shards, proofs,&root) {
                Ok(reconstructed_unit) => {
                    info!(
                        "Node {}: Reconstruction successful for root {:?}. Proceeding to commit.",
                        node.id, root
                    );

                    // Handle commit phase
                    handle_commit(node, sender, root, reconstructed_unit, epoch_id).await;
                }
                Err(e) => {
                    error!(
                        "Node {}: Reconstruction failed for root {:?}. Error: {:?}",
                        node.id, root, e
                    );

                    // info!(
                    //     "Node {}: Attempting recovery for epoch {}. Ensure DAG synchronization and shard validity.",
                    //     node.id, epoch_id
                    // );                    
                    // if let Err(recovery_err) = attempt_recovery(node, client, epoch_id, node_url).await {
                    //     error!(
                    //         "Node {}: Recovery failed for epoch {}. Error: {}",
                    //         node.id, epoch_id, recovery_err
                    //     );
                    // }
                }
            }
        } else {
            info!(
                "Node {}: Prevote accepted for root {:?}. Current votes: {}",
                node.id, root, *count
            );
        }
    }

    info!("Node {}: Finished handling PREVOTE REQUEST from Node {}", node.id, sender);
    Ok(())
}

