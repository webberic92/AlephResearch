use std::sync::Arc;
use axum::Json;
use base64::{engine::general_purpose, Engine};
use sha2::Digest;
use tokio::sync::RwLock;
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
        dag_utils::ensure_dag_synchronization,
        merkle_utils::validate_merkle_branch,
    },
};

/// Handles an incoming proposal request in the ch-RBC protocol.
pub async fn handle_propose(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> (StatusCode, Json<Response>) {
    // Log the incoming request
    {
        let node_state = node.read().await;
        info!(
            "*** Handling PROPOSE REQUEST: Node {} {} from Sender {} ***",
            node_state.id, node_state.ip_address, propose_request.base.sender_id
        );
    }

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
                node.read().await.id,
                index,
                propose_request.base.epoch_id,
                propose_request.base.root
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
        node.read().await.id,
        node.read().await.ip_address,
        propose_request.base.epoch_id
    );

        // --- Step 5: Update proposal tracker ---
        {
            let  node_state = node.write().await;
            let mut proposal_tracker = node_state.proposal_tracker.lock().await;
            proposal_tracker.insert(propose_request.base.sender_id);
    
            info!(
                "Node {}: Updated proposal tracker. Current proposals for epoch {}: {:?}",
                node_state.id, propose_request.base.epoch_id, *proposal_tracker
            );
        }

   // --- Step 6: Ensure DAG synchronization ---
let ip_address = {
    let node_state = node.read().await;
    node_state.ip_address.clone() // Clone the IP address to avoid borrowing issues
};

if let Err(e) = ensure_dag_synchronization(
    node.clone(), // Ensure the node is cloned appropriately
    &client,
    propose_request.base.epoch_id,
    &propose_request.base.sender_id,
    ip_address, // Use the cloned IP address here
)
.await
{
    let error_message = format!(
        "Node {}: DAG synchronization failed in handle_propose with node {}. Error: {:?}",
        node.read().await.id, // Access node ID separately
        propose_request.base.sender_id,
        e
    );
    error!("{}", error_message);
    return (
        StatusCode::BAD_REQUEST,
        Json(Response { status: error_message }),
    );
}

     // --- Step 7: Prevote transition (if quorum is reached) ---
     {
        let node_state = node.read().await;
        if node_state.is_quorum_reached(propose_request.base.epoch_id).await {
            info!(
                "Node {}: Quorum reached for epoch {}. Transitioning to prevote.",
                node_state.id, propose_request.base.epoch_id
            );


            let prevote_request = PrevoteRequest {
                propose: ProposeRequest {
                    base: BaseRequest {
                        sender_id: node_state.id,                     // Node's ID
                        root: propose_request.base.root.clone(),      // Root received from the original proposal
                        epoch_id: propose_request.base.epoch_id,      // Epoch ID from the original proposal
                    },
                    proofs: propose_request.proofs.clone(),           // Cloned proofs from the proposal
                    shards: propose_request.shards.clone(),           // Cloned shards from the proposal
                },
                sender_url: node_state.ip_address.clone(),            // Node's IP address
            };


            let (status_code, response) =
                handle_prevote(node.clone(), client.clone(), prevote_request).await;

            if status_code != StatusCode::OK {
                return (status_code, response);
            }
        } else {
            info!(
                "Node {}: Waiting for more proposals for epoch {}.",
                node_state.id, propose_request.base.epoch_id
            );
        }
    }


    // --- Step 8: Return success response ---
    (
        StatusCode::OK,
        Json(Response {
            status: format!(
                "Node {}: Proposal accepted for epoch {} from sender {}",
                node.read().await.id,
                propose_request.base.epoch_id,
                propose_request.base.sender_id
            ),
        }),
    )
}
