use serde_json::json;
use tracing::{error, info};
use reqwest::Client;
use crate::{
    structs::{node::Node, requests::PrevoteRequest},
    utils::{
        config_util::{load_config, save_config},
        epoch_utils::{ensure_no_overlap, handle_sync_epoch},
        merkle_utils::validate_merkle_branch,
    },
};

// Persist the epoch round ID to the TOML file
async fn persist_epoch_round_id(node_id: usize, epoch_id: u64, config_path: &str) {
    let mut config = load_config(config_path);
    config.consensus.epoch_round_id = (epoch_id + 1) as usize;

    match save_config(config_path, &config) {
        Ok(_) => info!(
            "Node {}: Successfully saved updated epoch_round_id = {} to config file.",
            node_id, config.consensus.epoch_round_id
        ),
        Err(e) => error!(
            "Node {}: Failed to save updated epoch_round_id to config file. Error: {:?}",
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
            "Node {}: Successfully saved updated proposal tracker to config file: {:?}",
            node_id, config.network.proposals
        ),
        Err(e) => error!(
            "Node {}: Failed to save updated proposal tracker to config file. Error: {:?}",
            node_id, e
        ),
    }
}

pub async fn handle_propose(
    node: &Node,
    client: &Client,
    sender: usize,
    root: Vec<u8>,
    proof: Vec<Vec<u8>>,
    shard: Vec<u8>,
    epoch_id: u64,
) {
    info!("Node {}: ==== Handling PROPOSE REQUEST from Node {} ====", node.id, sender);

    let sync_result = handle_sync_epoch(node, epoch_id, sender).await;
    let no_overlap_result = ensure_no_overlap(node, epoch_id).await;
    let computed_root = validate_merkle_branch(&shard, &proof);

    if sync_result.is_err() {
        error!(
            "Node {}: Propose phase failed for epoch {} due to synchronization error: {:?}",
            node.id, epoch_id, sync_result.err().unwrap()
        );
        return;
    }
    if no_overlap_result.is_err() {
        error!(
            "Node {}: Propose phase failed for epoch {} due to overlap detection: {:?}",
            node.id, epoch_id, no_overlap_result.err().unwrap()
        );
        return;
    }
    if computed_root != root {
        error!(
            "Node {}: Propose phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
        return;
    }

    info!(
        "Node {}: Propose Request VALIDATION successful for epoch {} from {}",
        node.id, epoch_id, sender
    );

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let mut proposal_tracker = config.network.proposals;

    proposal_tracker.push(sender);
    info!(
        "Node {}: Updated proposal_tracker after insertion: {:?}",
        node.id, proposal_tracker
    );

    if proposal_tracker.len() == config.node.total_nodes {
        info!("Node {}: All proposals received for epoch {}", node.id, epoch_id);

        // Synchronize epoch
        for node_url in &config.network.nodes {
            let payload = json!({ "epoch_id": epoch_id + 1 });
            match client
                .post(format!("http://{}/sync_epoch", node_url))
                .json(&payload)
                .send()
                .await
            {
                Ok(_) => info!(
                    "Node {}: Synchronized epoch {} with {}",
                    node.id, epoch_id + 1, node_url
                ),
                Err(e) => error!(
                    "Node {}: Failed to synchronize epoch with node {}. Error: {:?}",
                    node.id, node_url, e
                ),
            }
        }

        // Update and persist epoch round ID
        let mut epoch_tracker = node.epoch_round_id.lock().await;
        epoch_tracker.insert(epoch_id + 1);
        persist_epoch_round_id(node.id, epoch_id, "/home/aleph-node/aleph-node-config.toml").await;

        // Clear and persist proposal tracker
        proposal_tracker.clear();
        persist_proposal_tracker(node.id, &proposal_tracker, "/home/aleph-node/aleph-node-config.toml");

        // Transition to prevote phase
        info!("Node {}: Sending Prevotes to {:?}", node.id, &config.network.nodes);

        for node_url in &config.network.nodes {
            info!("Node {}: Preparing to send prevote to {}", node.id, node_url);

            let payload = PrevoteRequest {
                sender: node.id,
                root: root.clone(),
                epoch_id,
                unit: shard.clone(),
            };

            info!(
                "Node {}: Prevote payload: {{ sender: {}, root: {:?}, epoch_id: {} }}",
                node.id, payload.sender, payload.root, payload.epoch_id
            );

            match client
                .post(format!("http://{}/prevote", node_url))
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Node {}: Prevote successfully broadcasted to {}", node.id, node_url);
                    } else {
                        error!(
                            "Node {}: Prevote broadcast to {} failed with status: {}",
                            node.id, node_url, response.status()
                        );
                    }
                }
                Err(e) => error!(
                    "Node {}: Failed to send prevote to {}. Error: {:?}",
                    node.id, node_url, e
                ),
            }
        }
    } else {
        info!(
            "Node {}: Waiting for more proposals for epoch {}. Received: {}",
            node.id, epoch_id, proposal_tracker.len()
        );
    }
}
