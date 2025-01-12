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

// Persist the epoch round ID to the TOML file
async fn persist_epoch_round_id(node_id: usize, epoch_id: u64, config_path: &str) {
    let mut config = load_config(config_path);
    config.consensus.epoch_round_id = (epoch_id + 1) as usize;

    match save_config(config_path, &config) {
        Ok(_) => info!(
            "Node {} HANDLE PROPOSE: Successfully saved updated epoch_round_id = {} to config file.",
            node_id, config.consensus.epoch_round_id
        ),
        Err(e) => error!(
            "Node {} HANDLE PROPOSE: Failed to save updated epoch_round_id to config file. Error: {:?}",
            node_id, e
        ),
    }
}

// Persist the proposal tracker to the TOML file
fn persist_proposal_tracker(node_id: usize, proposal_tracker: &Vec<usize>, config_path: &str) {
    let mut config = load_config(config_path);
    config.network.proposals = proposal_tracker.clone();

    match save_config(config_path, &config) {
        Ok(_) => info!(
            "Node {} HANDLE PROPOSE: Successfully saved updated proposal tracker to config file: {:?}",
            node_id, config.network.proposals
        ),
        Err(e) => error!(
            "Node {} HANDLE PROPOSE: Failed to save updated proposal tracker to config file. Error: {:?}",
            node_id, e
        ),
    }
}

// Handle propose request
pub async fn handle_propose(
    node: &Node,
    client: &Client,
    sender: usize,
    root: Vec<u8>,
    proof: Vec<Vec<u8>>,
    shard: &Vec<u8>,
    epoch_id: u64,
) {
    info!("Node {} HANDLE PROPOSE: ==== Handling PROPOSE REQUEST from Node {} ====", node.id, sender);

    // Validate Merkle Branch
    let computed_root = validate_merkle_branch(&shard, &proof);
    if computed_root != root {
        error!(
            "Node {} HANDLE PROPOSE: Propose phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
        return;
    }

    if let Err(e) = reconstruct_unit(&[shard.to_vec()], &proof) {
        log_reconstruction_failure(node.id, epoch_id, &e);

        // Load configuration to access network nodes
        let config = load_config("/home/aleph-node/aleph-node-config.toml");

        if let Some(first_node_url) = config.network.nodes.get(0) {
            if let Err(recovery_err) = attempt_recovery(node, client, epoch_id, first_node_url).await {
                error!(
                    "Node {} HANDLE PROPOSE: Recovery failed for epoch {}. Error: {}",
                    node.id, epoch_id, recovery_err
                );
            }
        } else {
            error!(
                "Node {} HANDLE PROPOSE: Recovery failed for epoch {} due to missing network node configuration.",
                node.id, epoch_id
            );
        }
        return;
    }
    info!("Node {} HANDLE PROPOSE: Successfully reconstructed unit for epoch {}", node.id, epoch_id);


    // Synchronize and validate epoch
    if let Err(e) = handle_sync_epoch(node, epoch_id, sender).await {
        error!("Node {} HANDLE PROPOSE: Synchronization failed for epoch {}. Error: {:?}", node.id, epoch_id, e);
        return;
    }
    if let Err(e) = ensure_no_overlap(node, epoch_id).await {
        error!("Node {} HANDLE PROPOSE: Overlap detected for epoch {}. Error: {:?}", node.id, epoch_id, e);
        return;
    }

    info!("Node {} HANDLE PROPOSE: Propose validated for epoch {} from {}", node.id, epoch_id, sender);

    // Update proposal tracker
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let mut proposal_tracker = config.network.proposals.clone();
    info!("Node {} HANDLE PROPOSE: Adding proposal to proposal tracker  CURRENT: {:?} adding {}", node.id, proposal_tracker, sender);
    proposal_tracker.push(sender);
    persist_proposal_tracker(node.id, &proposal_tracker, "/home/aleph-node/aleph-node-config.toml");
    info!("Node {} HANDLE PROPOSE: Added proposal to proposal tracker  CURRENT: {:?}", node.id, proposal_tracker);

    if proposal_tracker.len() == config.node.total_nodes {
        info!("Node {} HANDLE PROPOSE: All proposals received for epoch {}", node.id, epoch_id);

        // Synchronize next epoch and DAG
        for node_url in &config.network.nodes {
            let payload = json!({ "epoch_id": epoch_id + 1 });
            if let Err(e) = client.post(format!("http://{}/sync_epoch", node_url)).json(&payload).send().await {
                error!("Node {} HANDLE PROPOSE: Failed to synchronize with node {}. Error: {:?}", node.id, node_url, e);
            } else {
                info!("Node {} HANDLE PROPOSE: Synchronized epoch {} with {}", node.id, epoch_id + 1, node_url);
            }

            // DAG Synchronization Check
            if let Err(e) = ensure_dag_synchronization(client, epoch_id, node_url).await {
                error!("Node {} HANDLE PROPOSE: DAG synchronization failed. Error: {:?}", node.id, e);
                return;
            } else {
                info!("Node {} HANDLE PROPOSE: DAG synchronized {} with {}", node.id, epoch_id + 1, node_url);

            }

        }

        // Update epoch tracker and persist
        let mut epoch_tracker = node.epoch_round_id.lock().await;
        epoch_tracker.insert(epoch_id + 1);
        persist_epoch_round_id(node.id, epoch_id, "/home/aleph-node/aleph-node-config.toml").await;

        // Clear proposal tracker and persist
        proposal_tracker.clear();
        persist_proposal_tracker(node.id, &proposal_tracker, "/home/aleph-node/aleph-node-config.toml");

        // Transition to prevote phase
        for node_url in &config.network.nodes {
            info!("Node {} HANDLE PROPOSE: Sending prevote to {}", node.id, node_url);

            let payload = PrevoteRequest {
                sender: node.id,
                root: root.clone(),
                proof: proof.clone(),
                epoch_id: epoch_id,
                shard: shard.clone(),
                node_url: node_url.clone(),
            };

            match client.post(format!("http://{}/prevote", node_url)).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Node {} HANDLE PROPOSE: Prevote successfully sent to {}", node.id, node_url);
                    } else {
                        error!(
                            "Node {} HANDLE PROPOSE: Prevote to {} failed with status: {}",
                            node.id, node_url, response.status()
                        );
                    }
                }
                Err(e) => {
                    error!("Node {} HANDLE PROPOSE: Failed to send prevote to {}. Error: {:?}", node.id, node_url, e);
                }
            }
        }
    } else {
        info!("Node {} HANDLE PROPOSE: Waiting for more proposals for epoch {}", node.id, epoch_id);
    }
}
