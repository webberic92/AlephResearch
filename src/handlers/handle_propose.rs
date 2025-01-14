use serde_json::json;
use tracing::{error, info};
use reqwest::Client;
use crate::{
    structs::{node::Node, requests::PrevoteRequest},
    utils::{
        config_util::{load_config, save_config},
        dag_utils::ensure_dag_synchronization,
        epoch_utils::{ensure_no_overlap, handle_sync_epoch},
        errors_util::log_reconstruction_failure,
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
    proof: Vec<Vec<u8>>,
    shard: &Vec<u8>,
    epoch_id: u64,
) {
    info!(
        "***==== Handling PROPOSE REQUEST Node {}:  from {} ====***",
        node.id, sender
    );

    // Step 1: Validate the proposal (Merkle root and unit reconstruction)
    // ch-RBC proof: Ensures data integrity and prevents malicious data injection (line 7 of ch-RBC protocol).
    if !validate_proposal(node, client, &root, &proof, &shard, epoch_id).await {
        return;
    }

    // Step 2: Synchronize the epoch to ensure all nodes are aligned
    // ch-RBC proof: Guarantees that nodes are processing data from the same round (line 11).
    if !synchronize_epoch(node, epoch_id, sender).await {
        return;
    }

    // Step 3: Update the proposal tracker with the sender's ID
    // ch-RBC proof: Tracks received proposals, ensuring a majority quorum is reached (lines 12-13).
    update_proposal_tracker(node, sender, epoch_id).await;

    // Step 4: Check if all proposals for the current epoch have been received
    // If true, finalize the epoch and transition to the next phase
    if all_proposals_received().await {
        finalize_epoch(node, client, epoch_id, &root, &proof, shard).await;
    } else {
        info!(
            "Node {} HANDLE PROPOSE: Waiting for more proposals for epoch {}",
            node.id, epoch_id
        );
    }
}

// Validate the proposal (Merkle branch and reconstruction)
// Ensures the integrity of the proposed data using Merkle proofs and reconstructs the unit if necessary.
async fn validate_proposal(
    node: &Node,
    client: &Client,
    root: &Vec<u8>,
    proof: &Vec<Vec<u8>>,
    shard: &Vec<u8>,
    epoch_id: u64,
) -> bool {


    //TODO: Log any recovery attempts and confirm that they involve querying honest nodes as per the proof (lines 15-16)



    // Validate Merkle branch
    let computed_root = validate_merkle_branch(shard, proof);
    if computed_root != *root {
        error!(
            "Node {} HANDLE PROPOSE: Merkle root mismatch for epoch {}. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
        return false;
    }

    // Attempt to reconstruct the unit
    if let Err(e) = reconstruct_unit(&[shard.to_vec()], proof) {
        log_reconstruction_failure(node.id, epoch_id, &e);

        // Attempt recovery from the first node if reconstruction fails
        // ch-RBC proof: Ensures liveness by allowing recovery from honest nodes (lines 15-16).
        if !attempt_recovery_from_first_node(node, client, epoch_id).await {
            return false;
        }
    }

    info!(
        "Node {} HANDLE PROPOSE: Successfully validated proposal for epoch {}",
        node.id, epoch_id
    );
    true
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
                "Node {} HANDLE PROPOSE: Recovery failed for epoch {}. Error: {}",
                node.id, epoch_id, e
            );
            return false;
        }
        true
    } else {
        error!(
            "Node {} HANDLE PROPOSE: Missing network node configuration for recovery in epoch {}",
            node.id, epoch_id
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
            "Node {} HANDLE PROPOSE: Synchronization failed for epoch {}",
            node.id, epoch_id
        );
        return false;
    }

    if ensure_no_overlap(node, epoch_id).await.is_err() {
        error!(
            "Node {} HANDLE PROPOSE: Overlap detected for epoch {}",
            node.id, epoch_id
        );
        return false;
    }

    info!(
        "Node {} HANDLE PROPOSE: Epoch {} synchronized successfully",
        node.id, epoch_id
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
    persist_proposal_tracker(node.id, &proposal_tracker, config_path);
    info!(
        "Node {} HANDLE PROPOSE: Updated proposal tracker for epoch {}: {:?}",
        node.id, epoch_id, proposal_tracker
    );
}

// Check if all proposals have been received
// ch-RBC proof: Ensures a majority quorum (2f+1) of proposals before moving to prevote.
// Check if a majority quorum (2f + 1) of proposals has been received
//TODO: However, the logic in all_proposals_received does not explicitly verify that the proposals come from distinct, non-faulty nodes.
//TODO: Add logic to ensure proposals are unique and originate from different nodes.
//TODO: Include safeguards to handle malicious nodes attempting to spam invalid proposals.
async fn all_proposals_received() -> bool {
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let total_nodes = config.node.total_nodes;
    let faulty_nodes = (total_nodes - 1) / 3; // f = ⌊(N-1)/3⌋
    let required_quorum = 2 * faulty_nodes + 1; //2F+1

    config.network.proposals.len() >= required_quorum
}

// Finalize the epoch, sync DAG, and transition to prevote
// This function performs the final steps of the propose phase and transitions to the prevote phase.
//TODO 
// Ensure retries for DAG synchronization in case of temporary network failures.
// Include a timeout mechanism to prevent indefinite delays caused by slow or unresponsive nodes.
async fn finalize_epoch(
    node: &Node,
    client: &Client,
    epoch_id: u64,
    root: &Vec<u8>,
    proof: &Vec<Vec<u8>>,
    shard: &Vec<u8>,
) {
    info!(
        "Node {} HANDLE PROPOSE: Finalizing epoch {}",
        node.id, epoch_id
    );

    let config = load_config("/home/aleph-node/aleph-node-config.toml");

    for node_url in &config.network.nodes {
        info!(
            "Node {} NETWORK NODES LOOP NODE URL : {}",
            node.id, node_url
        );
        
        synchronize_dag_and_epoch(node, client, epoch_id, node_url).await;
        send_prevote(node, client, node_url, epoch_id, root, proof, shard).await;
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
        error!("Failed to synchronize epoch {} with {}. Error: {:?}", epoch_id, node_url, e);
    } else {
        info!("Synchronized epoch {} with {}", epoch_id, node_url);
    }

    if let Err(e) = ensure_dag_synchronization(node, client, epoch_id, node_url).await {
        error!("DAG synchronization failed with {} for epoch {}. Error: {:?}", node_url, epoch_id, e);
    } else {
        info!("DAG synchronized with {} for epoch {}", node_url, epoch_id);
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
    proof: &Vec<Vec<u8>>,
    shard: &Vec<u8>,
) {
    let payload = PrevoteRequest {
        sender: node.id,
        root: root.clone(),
        proof: proof.clone(),
        epoch_id,
        shard: shard.clone(),
        node_url: node_url.to_string(),
    };

    match client
        .post(format!("http://{}/prevote", node_url))
        .json(&payload)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            info!("Prevote sent to {}", node_url);
        }
        Ok(response) => {
            error!(
                "Failed to send prevote to {}. Status: {}",
                node_url, response.status()
            );
        }
        Err(e) => {
            error!("Failed to send prevote to {}. Error: {:?}", node_url, e);
        }
    }
}

// Update the epoch tracker
// ch-RBC proof: Keeps track of the current epoch and ensures nodes stay synchronized.
async fn update_epoch_tracker(node: &Node, epoch_id: u64) {
    let mut epoch_tracker = node.epoch_round_id.lock().await;
    epoch_tracker.insert(epoch_id + 1);
    persist_epoch_round_id(node.id, epoch_id, "/home/aleph-node/aleph-node-config.toml").await;
}

// Persist the epoch round ID to the TOML file
pub async fn persist_epoch_round_id(node_id: usize, epoch_id: u64, config_path: &str) {
    let mut config = load_config(config_path);
    config.consensus.epoch_round_id = epoch_id + 1;

    match save_config(config_path, &config) {
        Ok(_) => info!("Node {}: Successfully updated epoch_round_id = {}.", node_id, config.consensus.epoch_round_id),
        Err(e) => error!("Node {}: Failed to update epoch_round_id. Error: {:?}", node_id, e),
    }
}

// Persist the proposal tracker to the TOML file
pub fn persist_proposal_tracker(node_id: usize, proposal_tracker: &Vec<usize>, config_path: &str) {
    let mut config = load_config(config_path);
    config.network.proposals = proposal_tracker.clone();

    match save_config(config_path, &config) {
        Ok(_) => info!("Node {}: Successfully updated proposal tracker in toml.", node_id),
        Err(e) => error!("Node {}: Failed to update proposal tracker. Error: {:?}", node_id, e),
    }
}
