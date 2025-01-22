use axum::Json;
use base64::{engine::general_purpose, Engine};
use sha2::Digest;
use tracing::{error, info};
use reqwest::StatusCode;
use crate::{
    structs::{node::Node, requests::ProposeRequest, responses::Response},
    utils::{
        config_util::{are_enough_proposals_received, update_proposal_tracker},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

pub async fn handle_propose(
    node: &Node,
    request: ProposeRequest,
) -> (StatusCode, Json<Response>) {
    info!(
        "*** Handling PROPOSE REQUEST: Node {} {} from Sender {} ***",
        node.id, node.ip_address, request.sender
    );

    // Step 1: Decode Base64-encoded shards
    let decoded_shards: Vec<Vec<u8>> = match request
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
    {
        Ok(decoded) => decoded,
        Err(e) => {
            let error_message = format!("Failed to decode shards: {:?}", e);
            error!("{}", error_message);
            return (
                StatusCode::BAD_REQUEST,
                Json(Response { status: error_message }),
            );
        }
    };

    // Log decoded shards
    info!(
        "Node {}: Decoded shards (Epoch {}): {:?}",
        node.id, request.epoch_id, decoded_shards
    );

    // Step 2: Decode Base64-encoded proofs
    let decoded_proofs: Vec<Vec<Vec<u8>>> = match request
    .proofs
    .iter()
    .map(|proof| {
        proof
            .iter()
            .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<_>, _>>() // Decode individual branch
    })
    .collect::<Result<Vec<_>, _>>() // Collect all decoded branches
{
    Ok(decoded) => decoded,
    Err(e) => {
        let error_message = format!("Failed to decode proofs: {:?}", e);
        error!("{}", error_message);
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }
};

    // Log decoded proofs
    info!(
        "Node {}: Decoded proofs (Epoch {}): {:?}",
        node.id, request.epoch_id, decoded_proofs
    );

    // Step 3: Compute shard hashes
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // Log shard hashes
    info!(
        "Node {}: Computed shard hashes (Epoch {}): {:?}",
        node.id, request.epoch_id, shard_hashes
    );

    // Step 4: Validate Merkle branches for each shard
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &request.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {} in epoch {}. Expected root: {:?}",
                node.id, index, request.epoch_id, request.root
            );
            error!("{}", error_message);
            return (
                StatusCode::BAD_REQUEST,
                Json(Response { status: error_message }),
            );
        }
    }

    info!(
        "Node {} {}: All Merkle branches validated successfully for epoch {}",
        node.id, node.ip_address, request.epoch_id
    );

    // Step 5: Attempt to reconstruct the original data
    match reconstruct_unit(&decoded_shards, &decoded_proofs, &request.root) {
        Ok(reconstructed_data) => {
            info!(
                "Node {} {}: Reconstruction successful for epoch {}. Reconstructed data: {:?}",
                node.id, node.ip_address, request.epoch_id, reconstructed_data
            );
        }
        Err(error_message) => {
            error!(
                "Node {} {}: Reconstruction failed for epoch {}: {}",
                node.id, node.ip_address, request.epoch_id, error_message
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(Response { status: error_message }),
            );
        }
    }

    // Step 6: Update the proposal tracker
    // Step 6: Update the proposal tracker
    update_proposal_tracker(node, request.sender, request.epoch_id).await;

    info!(
        "Node {}: Successfully updated proposal tracker for sender {} in epoch {}",
        node.id, request.sender, request.epoch_id
    );


    // Step 7: Check if enough proposals are received
    if are_enough_proposals_received().await {
        info!(
            "Node {} {}: Transitioning to PREVOTE phase for epoch {}.",
            node.id, node.ip_address, request.epoch_id
        );
    } else {
        info!(
            "Node {} {}: Waiting for more proposals for epoch {}.",
            node.id, node.ip_address, request.epoch_id
        );
    }

    // Step 8: Return success response
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
