use axum::Json;
use tracing::{error, info};
use reqwest::{Client, StatusCode};
use crate::{
    structs::{node::Node, responses::Response},
    utils::{
        config_util::{are_enough_proposals_received, update_proposal_tracker},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

/// Handles a PROPOSE request in the ch-RBC protocol.
/// 
/// This function performs the following steps:
/// 1. Validates the Merkle root using the provided shards and proofs.
/// 2. Attempts to reconstruct the original transaction from the provided data.
/// 3. Updates the proposal tracker for the current node.
/// 4. Transitions to the PREVOTE phase if enough proposals have been received.
pub async fn handle_propose(
    node: &Node,
    client: &Client,
    sender: usize,
    root: Vec<u8>,
    proofs: &[Vec<Vec<u8>>],
    shards: &[Vec<u8>],
    epoch_id: u64,
) -> (StatusCode, Json<Response>) {
    info!(
        "***==== Handling PROPOSE REQUEST Node {} {} from Sender {} ====***",
        node.id, node.ip_address, sender
    );
    info!("Proofs received for validation: {:?}", proofs);

    // Step 1: Validate the Merkle root
    let computed_root = validate_merkle_branch(shards, proofs);
    if computed_root != *root {
        let error_message = format!(
            "Node {} {}: Merkle root mismatch for epoch {}. Computed: {:?}, Expected: {:?}",
            node.id, node.ip_address, epoch_id, computed_root, root
        );
        error!("{}", error_message);
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }
    info!(
        "Node {} {}: Merkle root validation passed for epoch {}",
        node.id, node.ip_address, epoch_id
    );

    // Step 2: Validate reconstruction
    if let Err(error_message) = reconstruct_unit(shards, proofs, &root) {
        error!(
            "Node {} {}: Reconstruction failed for epoch {}: {}",
            node.id, node.ip_address, epoch_id, error_message
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // Step 3: Update the proposal tracker
    info!(
        "Node {} {}: Updating proposal tracker for sender {} and epoch {}",
        node.id, node.ip_address, sender, epoch_id
    );
    update_proposal_tracker(node, sender, epoch_id).await;

    // Step 4: Transition to the PREVOTE phase if enough proposals are received
    if are_enough_proposals_received().await {
        info!(
            "Node {} {}: Enough proposals received for epoch {}. Transitioning to PREVOTE phase.",
            node.id, node.ip_address, epoch_id
        );
        // Transition logic to PREVOTE phase (commented for now)
        // send_prevotes(node, client, epoch_id, &root, &proofs_vec, &shards_vec).await;
    } else {
        info!(
            "Node {} {}: Waiting for more proposals for epoch {}",
            node.id, node.ip_address, epoch_id
        );
    }

    // Response for a successful proposal handling
    (
        StatusCode::OK,
        Json(Response {
            status: format!(
                "Node {}: Proposal accepted for epoch {} from sender {}",
                node.id, epoch_id, sender
            ),
        }),
    )
}

