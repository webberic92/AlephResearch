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

// Top-level function to handle a propose request
// This function implements the propose phase of ch-RBC, ensuring that a proposal is validated, synchronized, and processed correctly.
// It also handles DAG synchronization and transitions to the prevote phase if all conditions are met.
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
        "***==== Handling PROPOSE REQUEST Node {} {} :  from {} ====***",
        node.id, node.ip_address, sender
    );

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
    } else {
        info!(
            "Node {} {}: Merkle root validation passed for epoch {}",
            node.id, node.ip_address, epoch_id
        );
    }   
    
    // Step 2: Validate reconstruction
    if let Err(error_message) = reconstruct_unit(shards,proofs,&root) {
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // Step 3: Update the proposal tracker
    update_proposal_tracker(node, sender, epoch_id).await;

    // Step 4: Transition to the prevote phase if enough proposals are received
    if are_enough_proposals_received().await {

        // send_prevotes(node, client, epoch_id, &root, &proofs_vec, &shards_vec).await;
    } else {
        info!(
            "Node {} {}: Waiting for more proposals for epoch {}",
            node.id, node.ip_address, epoch_id
        );
    }

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



