use axum::Json;
use serde_json::json;
use tracing::{error, info};
use reqwest::{Client, StatusCode};
use crate::{
    structs::{node::Node, requests::PrevoteRequest, responses::Response},
    utils::{
        config_util::{are_enough_proposals_received, load_config, save_config},
        dag_utils::ensure_dag_synchronization,
        epoch_utils::{ensure_no_overlap, handle_sync_epoch},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
        recovery_util::attempt_recovery,
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

    // Step 1: Validate the Merkle root
    if let Err(error_message) = validate_merkle_branch_proposal(node, &root, proofs, shards, epoch_id).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // Step 2: Validate reconstruction
    if let Err(error_message) = validate_and_reconstruct_unit(node, client, &root, proofs, shards, epoch_id).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(Response { status: error_message }),
        );
    }

    // Step 3: Synchronize the epoch
    if !synchronize_epoch(node, epoch_id, sender).await {
        error!(
            "Node {} {}: Synchronization failed for epoch {} from sender {}",
            node.id, node.ip_address, epoch_id, sender
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(Response {
                status: format!(
                    "Node {}: Synchronization failed for epoch {} from sender {}",
                    node.id, epoch_id, sender
                ),
            }),
        );
    }

    // Step 3: Update the proposal tracker
    update_proposal_tracker(node, sender, epoch_id).await;

    // Step 4: Transition to the prevote phase if enough proposals are received
    if are_enough_proposals_received().await {
        info!(
            "Node {} {}: Received enough proposals to PREVOTE from handle_propose!",
            node.id, node.ip_address
        );

        // let proofs_vec = proofs.to_vec();
        // let shards_vec = shards.to_vec();

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



// Validate the proposal (Merkle branch and reconstruction)
// Ensures the integrity of the proposed data using Merkle proofs and reconstructs the unit if necessary.
// async fn validate_proposal(
//     node: &Node,
//     client: &Client,
//     root: &Vec<u8>,
//     proofs: &[Vec<Vec<u8>>], // Updated type to match `reconstruct_unit`
//     shards: &[Vec<u8>], // Updated type to match `reconstruct_unit`
//     epoch_id: u64,
// ) -> bool {
//     info!(
//         "Node {} {}: Starting proposal validation for epoch {}",
//         node.id, node.ip_address, epoch_id
//     );

//     // Validate Merkle branch
//     let computed_root = validate_merkle_branch(shards,proofs); // Pass slices directly
//     if computed_root != *root {
//         error!(
//             "Node {} {}: HANDLE PROPOSE: Merkle root mismatch for epoch {}. Computed: {:?}, Expected: {:?}",
//             node.id, node.ip_address, epoch_id, computed_root, root
//         );
//         return false;
//     }
//     info!(
//         "Node {} {}: Merkle root validation passed for epoch {}",
//         node.id, node.ip_address, epoch_id
//     );

//     // Attempt to reconstruct the unit
//     match reconstruct_unit(shards, proofs,root) {
//         Ok(_) => {
//             info!(
//                 "Node {} {}: Successfully reconstructed unit for epoch {}",
//                 node.id, node.ip_address, epoch_id
//             );
//         }
//         Err(e) => {
//             log_reconstruction_failure(node.id, epoch_id, &e);

//             // Attempt recovery from the first node if reconstruction fails
//             info!(
//                 "Node {} {}: Attempting recovery from the first node for epoch {}",
//                 node.id, node.ip_address, epoch_id
//             );
//             if !attempt_recovery_from_first_node(node, client, epoch_id).await {
//                 error!(
//                     "Node {} {}: Recovery failed for epoch {}",
//                     node.id, node.ip_address, epoch_id
//                 );
//                 return false;
//             }
//         }
//     }

//     info!(
//         "Node {} {}: Successfully validated proposal for epoch {}",
//         node.id, node.ip_address, epoch_id
//     );
//     true
// }

async fn validate_merkle_branch_proposal(
    node: &Node,
    root: &Vec<u8>,
    proofs: &[Vec<Vec<u8>>],
    shards: &[Vec<u8>],
    epoch_id: u64,
) -> Result<(), String> {
    info!(
        "Node {} {}: Validating Merkle branch for epoch {}",
        node.id, node.ip_address, epoch_id
    );

    let computed_root = validate_merkle_branch(shards, proofs);
    if computed_root != *root {
        let error_message = format!(
            "Node {} {}: Merkle root mismatch for epoch {}. Computed: {:?}, Expected: {:?}",
            node.id, node.ip_address, epoch_id, computed_root, root
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {} {}: Merkle root validation passed for epoch {}",
        node.id, node.ip_address, epoch_id
    );

    Ok(())
}

async fn validate_and_reconstruct_unit(
    node: &Node,
    client: &Client,
    root: &Vec<u8>,
    proofs: &[Vec<Vec<u8>>],
    shards: &[Vec<u8>],
    epoch_id: u64,
) -> Result<(), String> {
    info!(
        "Node {} {}: Reconstructing unit for epoch {}",
        node.id, node.ip_address, epoch_id
    );

    match reconstruct_unit(shards, proofs, root) {
        Ok(_) => {
            info!(
                "Node {} {}: Successfully reconstructed unit for epoch {}",
                node.id, node.ip_address, epoch_id
            );
            Ok(())
        }
        Err(e) => {
            error!(
                "Node {}: Reconstruction failed for root {:?}. Error: {:?}",
                node.id, root, e
            );
            // Attempt recovery
            // info!(
            //     "Node {} {}: Attempting recovery from the first node for epoch {}",
            //     node.id, node.ip_address, epoch_id
            // );
            // if !attempt_recovery_from_first_node(node, client, epoch_id).await {
            //     let error_message = format!(
            //         "Node {} {}: Recovery failed for epoch {}",
            //         node.id, node.ip_address, epoch_id
            //     );
            //     error!("{}", error_message);
            //     return Err(error_message);
            // }
            Err(format!(
                "Node {} {}: Reconstruction failed for epoch {}: {}",
                node.id, node.ip_address, epoch_id, e
            ))
        }
    }
}


// Attempt recovery from the first node in the network
async fn attempt_recovery_from_first_node(
    node: &Node,
    client: &Client,
    epoch_id: u64,
) -> bool {
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    if let Some(first_node_url) = config.network.nodes.get(0) {
        if let Err(e) = attempt_recovery(node, client, epoch_id, first_node_url).await {
            error!(
                "Node {} {}  HANDLE PROPOSE: Recovery failed for epoch {}. Error: {}",
                node.id, node.ip_address, epoch_id, e
            );
            return false;
        }
        true
    } else {
        error!(
            "Node {} {}  HANDLE PROPOSE: Missing network node configuration for recovery in epoch {}",
            node.id, node.ip_address, epoch_id
        );
        false
    }
}

// Synchronize and validate epoch
// Ensures all nodes are processing the same epoch and no duplicate processing occurs.
async fn synchronize_epoch(node: &Node, epoch_id: u64, sender: usize) -> bool {

    // #TODO Ensure that the epoch tracker properly accounts for completed epochs to avoid processing outdated proposals.

    if handle_sync_epoch(node, epoch_id, sender).await.is_err() {
        error!(
            "Node {} {}  HANDLE PROPOSE: Synchronization failed for epoch {}",
           node.id, node.ip_address, epoch_id
        );
        return false;
    }

    if ensure_no_overlap(node, epoch_id).await.is_err() {
        error!(
            "Node {} {}  HANDLE PROPOSE: Overlap detected for epoch {}",
           node.id, node.ip_address, epoch_id
        );
        return false;
    }

    info!(
        "Node {} {}  HANDLE PROPOSE: Epoch {} synchronized successfully",
       node.id, node.ip_address, epoch_id
    );
    true
}

// Update the proposal tracker
// Tracks which nodes have submitted valid proposals to ensure quorum.
async fn update_proposal_tracker(node: &Node, sender: usize, epoch_id: u64) {
    let config_path = "/home/aleph-node/aleph-node-config.toml";
    let config = load_config(config_path);
    let mut proposal_tracker = config.network.proposals.clone();
    proposal_tracker.push(sender);
    persist_proposal_tracker( &proposal_tracker, config_path);
    info!(
        "Node {} {}  HANDLE PROPOSE: Updated proposal tracker for epoch {}: {:?}",
        node.id, node.ip_address, epoch_id, proposal_tracker
    );
}


// Finalize the epoch, sync DAG, and transition to prevote
// This function performs the final steps of the propose phase and transitions to the prevote phase.
//TODO 
// Ensure retries for DAG synchronization in case of temporary network failures.
// Include a timeout mechanism to prevent indefinite delays caused by slow or unresponsive nodes.
async fn send_prevotes(
    node: &Node,
    client: &Client,
    epoch_id: u64,
    root: &Vec<u8>,
    proofs: &Vec<Vec<Vec<u8>>>,
    shards: &Vec<Vec<u8>>,
) {
    info!(
        "Node {} {} All Proposals received: Finalizing epoch {}",
        node.id, node.ip_address, epoch_id
    );

    let config = load_config("/home/aleph-node/aleph-node-config.toml");

    for node_url in &config.network.nodes {
        info!(
            "Node {} {} NETWORK NODES LOOP NODE URL : {}",
            node.id,node.ip_address, node_url
        );
        
        synchronize_dag_and_epoch(node, client, epoch_id, node_url).await;
        send_prevote(node, client, node_url, epoch_id, root, proofs, shards).await;
    }

    update_epoch_tracker(node, epoch_id).await;
}

// Synchronize DAG and next epoch
// Ensures all nodes have a consistent view of the DAG before moving to the next epoch.
async fn synchronize_dag_and_epoch(node: &Node, client: &Client, epoch_id: u64, node_url: &String) {
    let payload = json!({ "epoch_id": epoch_id });
    if let Err(e) = client
        .post(format!("http://{}/sync_epoch", node_url))
        .json(&payload)
        .send()
        .await
    {
        error!("Node {} {} Failed to synchronize epoch {} with {}. Error: {:?}",  node.id,node.ip_address,epoch_id, node_url, e);
    } else {
        info!("Node {} {} Synchronized epoch {} with {}",  node.id,node.ip_address,epoch_id, node_url);
    }

    if let Err(e) = ensure_dag_synchronization(node, client, epoch_id, node_url).await {
        error!("Node {} {} DAG synchronization failed with {} for epoch {}. Error: {:?}",  node.id,node.ip_address,node_url, epoch_id, e);
    } else {
        info!("Node {} {} DAG synchronized with {} for epoch {}",  node.id,node.ip_address,node_url, epoch_id);
    }
}

// Send prevote request
// // Sends a prevote message to all nodes as part of the prevote phase.
// TODO: Log whether the prevote messages are acknowledged by the recipient nodes for traceability.
//  TODO: Add mechanisms to handle and retry failed prevote transmissions.
async fn send_prevote(
    node: &Node,
    client: &Client,
    node_url: &str,
    epoch_id: u64,
    root: &Vec<u8>,
    proofs: &Vec<Vec<Vec<u8>>>,
    shards: &Vec<Vec<u8>>,
) {

//     pub shards: Vec<Vec<u8>>, // Multiple shards
// pub proofs: Vec<Vec<Vec<u8>>>,
    let payload = PrevoteRequest {
        sender: node.id,
        root: root.clone(),
        proofs: proofs.clone(),
        epoch_id,
        shards: shards.clone(),
        node_url: node_url.to_string(),
    };

    match client
        .post(format!("http://{}/prevote", node_url))
        .json(&payload)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            info!("Node {} {} Prevote sent to {}", node.id,node.ip_address, node_url);
        }
        Ok(response) => {
            error!(
                "Node {} {} Failed to send prevote to {}. Status: {}",
                node.id,node.ip_address,node_url, response.status()
            );
        }
        Err(e) => {
            error!("Node {} {} Failed to send prevote to {}. Error: {:?}", node.id,node.ip_address, node_url, e);
        }
    }
}

// Update the epoch tracker
// ch-RBC proof: Keeps track of the current epoch and ensures nodes stay synchronized.
async fn update_epoch_tracker(node: &Node, epoch_id: u64) {
    let mut epoch_tracker = node.epoch_round_id.lock().await;
    epoch_tracker.insert(epoch_id + 1);
    persist_epoch_round_id( epoch_id, "/home/aleph-node/aleph-node-config.toml").await;
}

// Persist the epoch round ID to the TOML file
pub async fn persist_epoch_round_id(epoch_id: u64, config_path: &str) {
    let mut config = load_config(config_path);
    config.consensus.epoch_round_id = epoch_id + 1;

    match save_config(config_path, &config) {
        Ok(_) => info!("Node {} {} Successfully updated epoch_round_id = {}.", config.node.id, config.network.ip_address, config.consensus.epoch_round_id),
        Err(e) => error!("Node {} {} Failed to update epoch_round_id. Error: {:?}", config.node.id, config.network.ip_address, e),
    }
}

// Persist the proposal tracker to the TOML file
pub fn persist_proposal_tracker(proposal_tracker: &Vec<usize>, config_path: &str) {
    let mut config = load_config(config_path);
    config.network.proposals = proposal_tracker.clone();

    match save_config(config_path, &config) {
        Ok(_) => info!("Node {} {} Successfully updated proposal tracker in toml.", config.node.id, config.network.ip_address),
        Err(e) => error!("Node {} {} Failed to update proposal tracker. Error: {:?}", config.node.id, config.network.ip_address, e),
    }
}
