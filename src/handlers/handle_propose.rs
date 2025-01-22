use axum::Json;
use base64::{engine::general_purpose, Engine};
use tracing::{error, info};
use reqwest::{Client, StatusCode};
use crate::{
    structs::{node::Node, requests::ProposeRequest, responses::Response},
    utils::{
        config_util::{are_enough_proposals_received, update_proposal_tracker},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

pub async fn handle_propose(
    node: &Node,
    client: &Client,
    request: ProposeRequest, // Accept parsed ProposeRequest
) -> (StatusCode, Json<Response>) {
    info!(
        "***==== Handling PROPOSE REQUEST Node {} {} from Sender {} ====***",
        node.id, node.ip_address, request.sender
    );
    info!("Encoded proofs received for validation: {:?}", request.proofs);

    // Decode Base64-encoded shards
    let decoded_shards: Vec<Vec<u8>> = match request
    .shards
    .iter()
    .map(|shard| general_purpose::STANDARD.decode(shard))
    .collect::<Result<Vec<_>, _>>()
{
    Ok(decoded) => {
        info!("Decoded shards: {:?}", decoded);
        decoded
    }
    Err(e) => {
        let error_message = format!("Failed to decode shards: {:?}", e);
        error!("{}", error_message);
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }
};

let decoded_proofs: Vec<Vec<Vec<u8>>> = match request
    .proofs
    .iter()
    .map(|proof| {
        proof
            .iter()
            .map(|p| general_purpose::STANDARD.decode(p))
            .collect::<Result<Vec<_>, _>>()
    })
    .collect::<Result<Vec<_>, _>>()
{
    Ok(decoded) => {
        info!("Decoded proofs: {:?}", decoded);
        decoded
    }
    Err(e) => {
        let error_message = format!("Failed to decode proofs: {:?}", e);
        error!("{}", error_message);
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }
};
    

    // Step 1: Validate the Merkle root
    // let computed_root = validate_merkle_branch(&decoded_shards, &decoded_proofs);
    // if computed_root != request.root {
    //     let error_message = format!(
    //         "Node {} {}: Merkle root mismatch for epoch {}. Computed: {:?}, Expected: {:?}",
    //         node.id, node.ip_address, request.epoch_id, computed_root, request.root
    //     );
    //     error!("{}", error_message);
    //     return (
    //         StatusCode::BAD_REQUEST,
    //         Json(Response { status: error_message }),
    //     );
    // }
    // info!(
    //     "Node {} {}: Merkle root validation passed for epoch {}",
    //     node.id, node.ip_address, request.epoch_id
    // );

    // Step 2: Validate reconstruction
    if let Err(error_message) = reconstruct_unit(&decoded_shards, &decoded_proofs, &request.root) {
        error!(
            "Node {} {}: Reconstruction failed for epoch {}: {}",
            node.id, node.ip_address, request.epoch_id, error_message
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // Step 3: Update the proposal tracker
    info!(
        "Node {} {}: Updating proposal tracker for sender {} and epoch {}",
        node.id, node.ip_address, request.sender, request.epoch_id
    );
    update_proposal_tracker(node, request.sender, request.epoch_id).await;

    // Step 4: Transition to the PREVOTE phase if enough proposals are received
    if are_enough_proposals_received().await {
        info!(
            "Node {} {}: Enough proposals received for epoch {}. Transitioning to PREVOTE phase.",
            node.id, node.ip_address, request.epoch_id
        );
    } else {
        info!(
            "Node {} {}: Waiting for more proposals for epoch {}",
            node.id, node.ip_address, request.epoch_id
        );
    }

    (
        StatusCode::OK,
        Json(Response {
            status: format!(
                "Node {}: Proposal accepted for epoch {} from sender {}",
                node.id, request.epoch_id, request.sender
            ),
        }),
    )
}
