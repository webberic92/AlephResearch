use serde_json::json;
use tracing::{error, info};

use reqwest::Client;

use crate::{handlers::handle_prevote::handle_prevote, structs::{node::Node, requests::PrevoteRequest}, utils::{config_util::{load_config, save_config}, epoch_utils::{ensure_no_overlap, handle_sync_epoch}, merkle_utils::validate_merkle_branch}};

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
    
    let sync_result = handle_sync_epoch(node,epoch_id, sender).await;
    let no_overlap_result = ensure_no_overlap(node,epoch_id).await;
    let computed_root = validate_merkle_branch(&shard, &proof);
    
    // Log individual failures explicitly
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
    }

    // If everything is successful
    info!("Node {}: Propose phase successful for epoch {} from {}", node.id, epoch_id, sender);


    //load config
    let mut config = load_config("/home/aleph-node/aleph-node-config.toml");

    // Store the proposal for this epoch
    let mut proposal_tracker = config.network.proposals;
    info!("Node {}: Inserting sender {} into proposal_tracker {:?}", node.id, sender, proposal_tracker);

    proposal_tracker.push(sender);
    info!("Node {}: Inserted sender {} into proposal_tracker {:?}", node.id, sender, proposal_tracker);

    
    // Check if all proposals are received
    info!(
        "Node {}: config.node.total_nodes = {} config.totalnodes minus 1 (Removed logic) = {}. Received: {}",
        node.id, config.node.total_nodes, config.node.total_nodes-1, proposal_tracker.len());

        if proposal_tracker.len() == config.node.total_nodes {
            info!("Node {}: All proposals received for epoch {}", node.id, epoch_id);

            // Synchronize epoch
            for node_url in &config.network.nodes {
                let payload = json!({ "epoch_id": epoch_id + 1 });
                if let Err(e) = client
                    .post(format!("http://{}/sync_epoch", node_url))
                    .json(&payload)
                    .send()
                    .await
                {
                    error!("Failed to synchronize epoch with node {}: {:?}", node_url, e);
                } else {
                    info!("Node {}: Synchronized epoch {} with {}", node.id, epoch_id + 1, node_url);
                }
            }

            let mut tracker = node.epoch_tracker.lock().await;
            info!(
                "Node {}: epoch_tracker = {:?} inserting epoch_id = {}",
                node.id, tracker, epoch_id + 1);
            tracker.insert(epoch_id + 1);  // Move to next epoch
            
            info!(
                "Node {}: Clearing proposal_tracker {:?}",
                node.id, proposal_tracker);

            proposal_tracker.clear();
            // Transition to prevote phase
            handle_prevote(node,sender, root.clone(), epoch_id).await;
    
            // Broadcast prevote
            for node_url in &config.network.nodes {
                let payload = PrevoteRequest {
                    sender: node.id,
                    root: root.clone(),
                    epoch_id,
                };
                if let Err(e) = client.post(format!("http://{}/prevote", node_url))
                    .json(&payload)
                    .send()
                    .await
                {
                    error!("Failed to send prevote to node {}: {:?}", node_url, e);
                } else {
                    info!("Node {}: Prevote broadcasted to {}", node.id, node_url);
                }
            }
    
            // Clear tracker for next epoch
            proposal_tracker.clear();
        } else {
            info!(
                "Node {}: Waiting for more proposals for epoch {}. Received: {}",
                node.id, epoch_id, proposal_tracker.len()
            );
        }
        //Either is cleared or node is appended too proposal_tracker
        config.network.proposals=proposal_tracker;
        save_config("/home/aleph-node/aleph-node-config.toml", &config).unwrap();
}