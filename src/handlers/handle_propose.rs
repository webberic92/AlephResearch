use std::sync::Arc;
use axum::Json;
use base64::{engine::general_purpose, Engine};
use sha2::Digest;
use tracing::{error, info};
use reqwest::{Client, StatusCode};
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{
        node::Node,
        requests::{BaseRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    },
    utils::{
        config_util::{are_enough_proposals_received, update_proposal_tracker},
        dag_utils::ensure_dag_synchronization,
        merkle_utils::validate_merkle_branch,
    },
};

/// Handles an incoming proposal request in the ch-RBC protocol.
pub async fn handle_propose(
    node: Arc<Node>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> (StatusCode, Json<Response>) {
    // Log the incoming request
    info!(
        "*** Handling PROPOSE REQUEST: Node {} {} from Sender {} ***",
        node.id, node.ip_address, propose_request.base.sender_id
    );

    // --- Step 1: Decode Base64-encoded shards ---
    let decoded_shards: Vec<Vec<u8>> = match propose_request
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

    // --- Step 2: Decode Base64-encoded proofs ---
    let decoded_proofs: Vec<Vec<Vec<u8>>> = match propose_request
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

    // --- Step 3: Compute shard hashes ---
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // --- Step 4: Validate Merkle branches ---
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &propose_request.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {} in epoch {}. Expected root: {:?}",
                node.id, index, propose_request.base.epoch_id, propose_request.base.root
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
        node.id, node.ip_address, propose_request.base.epoch_id
    );

    // --- Step 5: Update proposal tracker ---
    update_proposal_tracker(&node, propose_request.base.sender_id, propose_request.base.epoch_id)
        .await;

    info!(
        "Node {}: Successfully updated proposal tracker for sender {} in epoch {}",
        node.id, propose_request.base.sender_id, propose_request.base.epoch_id
    );

    // --- Step 6: Ensure DAG synchronization ---
    if let Err(e) = ensure_dag_synchronization(
        &node,
        &client,
        propose_request.base.epoch_id,
        &propose_request.base.sender_id,
        &node.ip_address,
    )
    .await
    {
        let error_message = format!(
            "Node {}: DAG synchronization failed in handle_propose with node {}. Error: {:?}",
            node.id, propose_request.base.sender_id, e
        );
        error!("{}", error_message);
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // --- Step 7: Prevote transition (if quorum is reached) ---

        if are_enough_proposals_received().await {
            // Construct PrevoteRequest for Node 2
            let prevote_request = PrevoteRequest {
                propose: ProposeRequest {
                    base: BaseRequest {
                        sender_id: node.id,                     // This is Node 2's ID
                        root: propose_request.base.root.clone(), // This is the root received from Node 1
                        epoch_id: propose_request.base.epoch_id, // This is the epoch ID from Node 1
                    },
                    proofs: propose_request.proofs.clone(),     // Proofs from Node 1's proposal
                    shards: propose_request.shards.clone(),     // Shards from Node 1's proposal
                },
                sender_url: node.ip_address.clone(),            // This is Node 2's IP
            };
        
            // Transition to the Prevote phase
            handle_prevote(node.clone(), client.clone(), prevote_request).await;

    } else {
        info!(
            "Node {} {}: Waiting for more proposals for epoch {}.",
            node.id, node.ip_address, propose_request.base.epoch_id
        );
    }

    // --- Step 8: Return success response ---
    (
        StatusCode::OK,
        Json(Response {
            status: format!(
                "Node {}: Proposal accepted for epoch {} from sender {}",
                node.id, propose_request.base.epoch_id, propose_request.base.sender_id
            ),
        }),
    )
}
