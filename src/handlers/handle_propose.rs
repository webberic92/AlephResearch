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
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes())) // Convert String to &[u8]
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

    // Decode Base64-encoded proofs
    let decoded_proofs: Vec<Vec<u8>> = match request
        .proofs
        .iter()
        .flat_map(|proof| proof.iter().map(|p| general_purpose::STANDARD.decode(p.as_bytes())))
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

    // Validate the Merkle branch for each shard
    for (index, shard) in decoded_shards.iter().enumerate() {
        if !validate_merkle_branch(&decoded_shards, &decoded_proofs, index, &request.root) {
            let error_message = format!(
                "Node {} {}: Merkle root mismatch for shard {} in epoch {}. Expected root: {:?}",
                node.id, node.ip_address, index, request.epoch_id, request.root
            );
            error!("{}", error_message);
            return (
                StatusCode::BAD_REQUEST,
                Json(Response { status: error_message }),
            );
        }
    }
    info!(
        "Node {} {}: Merkle root validation passed for all shards in epoch {}",
        node.id, node.ip_address, request.epoch_id
    );

    // Validate reconstruction
    // if let Err(error_message) = reconstruct_unit(&decoded_shards, &decoded_proofs, &request.root) {
    //     error!(
    //         "Node {} {}: Reconstruction failed for epoch {}: {}",
    //         node.id, node.ip_address, request.epoch_id, error_message
    //     );
    //     return (
    //         StatusCode::BAD_REQUEST,
    //         Json(Response { status: error_message }),
    //     );
    // }

    // Update the proposal tracker
    info!(
        "Node {} {}: Updating proposal tracker for sender {} and epoch {}",
        node.id, node.ip_address, request.sender, request.epoch_id
    );
    update_proposal_tracker(node, request.sender, request.epoch_id).await;

    // Transition to PREVOTE phase if enough proposals are received
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


