use serde_json::json;
use tracing::{error, info};

use reqwest::Client;

use crate::{structs::{node::Node, requests::PrevoteRequest}, utils::{config_util::{load_config, save_config}, epoch_utils::{ensure_no_overlap, handle_sync_epoch}, merkle_utils::validate_merkle_branch}};

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
    info!("Node {}: Propose Request VALIDATION successful for epoch {} from {}", node.id, epoch_id, sender);


    //load config
    let mut config = load_config("/home/aleph-node/aleph-node-config.toml");

    // Store the proposal for this epoch
    let mut proposal_tracker = config.network.proposals;
    info!("Node {}: Inserting sender {} into proposal_tracker {:?}", node.id, sender, proposal_tracker);

    proposal_tracker.push(sender);
    info!("Node {}: Inserted sender {} into proposal_tracker {:?}", node.id, sender, proposal_tracker);

    
    // Check if all proposals are received
    info!(
        "Node {}: config.node.total_nodes = {} config.totalnodes - 1 = {}. Received: {}",
        node.id, config.node.total_nodes, config.node.total_nodes-1, proposal_tracker.len());

        if proposal_tracker.len() == config.node.total_nodes {
            info!("Node {}: ***===All proposals requests received for epoch {}", node.id, epoch_id);

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

            // let mut epoch_tracker = node.epoch_tracker.lock().await;
            // info!(
            //     "Node {}: UPDATING EPOCH : epoch_tracker = {:?} inserting epoch_id = {}",
            //     node.id, epoch_tracker, epoch_id + 1);
            // epoch_tracker.insert(epoch_id + 1);  // Move to next epoch
            
            // info!(
            //     "Node {}: Clearing proposal_tracker {:?}",
            //     node.id, proposal_tracker);

            // proposal_tracker.clear();

            // info!(
            //     "Node {}: Cleared proposal_tracker {:?}",
            //     node.id, proposal_tracker);
           
            
            // Transition to prevote phase
            // handle_prevote(node,sender, root.clone(), epoch_id).await;
            //TOOK THIS OUT FOR NOW.

            let mut epoch_tracker = node.epoch_round_id.lock().await;

            // Log the current state and the update
            info!(
                "Node {}: UPDATING EPOCH: Current epoch = {:?}, inserting epoch_id = {}",
                node.id, epoch_tracker, epoch_id + 1
            );
            
            // Update the epoch tracker
            epoch_tracker.insert(epoch_id + 1);
            
            // Log the proposal tracker before clearing
            info!(
                "Node {}: Clearing proposal_tracker: Current state = {:?}",
                node.id, proposal_tracker
            );
            
            // Clear the proposal tracker
            proposal_tracker.clear();
            
            // Log the cleared proposal tracker
            info!(
                "Node {}: Cleared proposal_tracker: Current state = {:?}",
                node.id, proposal_tracker
            );
            
            // Save updated epoch tracker back to the TOML configuration
            let mut config = load_config("/home/aleph-node/aleph-node-config.toml");
            config.consensus.epoch_round_id = (epoch_id + 1) as usize; // Assuming epoch is a field in consensus section
            
            if let Err(e) = save_config("/home/aleph-node/aleph-node-config.toml", &config) {
                error!(
                    "Node {}: Failed to save updated epoch to config file. Error: {:?}",
                    node.id, e
                );
            } else {
                info!(
                    "Node {}: Successfully saved updated epoch to config file: {:?}",
                    node.id, config.consensus.epoch_round_id
                );
            }

        
            info!(
                "Node {}: Sending Prevotes to {:?}",
                node.id, &config.network.nodes);
            // Broadcast prevote
            for node_url in &config.network.nodes {
                info!("Node {}: Preparing to send prevote to {}", node.id, node_url);
            
                // Construct the payload for the prevote request
                let payload = PrevoteRequest {
                    sender: node.id,
                    root: root.clone(),
                    epoch_id,
                };
            
                // Log the payload before sending
                info!(
                    "Node {}: Prevote payload: {{ sender: {}, root: {:?}, epoch_id: {} }}",
                    node.id, payload.sender, payload.root, payload.epoch_id
                );
            
                // Send the HTTP POST request
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
                    Err(e) => {
                        error!(
                            "Node {}: Failed to send prevote to {}. Error: {:?}",
                            node.id, node_url, e
                        );
                    }
                }
            
                info!("Node {}: Finished processing prevote for {}", node.id, node_url);
            }
    
        } else {
            info!(
                "Node {}: Waiting for more proposals for epoch {}. Received: {}",
                node.id, epoch_id, proposal_tracker.len()
            );
        }

        info!(
            "Node {}: Saving proposal_tracker back to config.network.proposals in toml",
            node.id);
        //Either is cleared or node is appended too proposal_tracker
        config.network.proposals=proposal_tracker;
        save_config("/home/aleph-node/aleph-node-config.toml", &config).unwrap();

}