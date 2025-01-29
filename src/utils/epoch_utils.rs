use std::sync::Arc;

use reqwest::Client;
use tracing::{error, info};

use crate::{structs::{node::Node, requests::{BaseRequest, CommitRequest, SyncEpochRequest}, toml_config::TomlConfig}, utils::config_util::{load_config, persist_epoch_round_id, save_config}};

pub async fn ensure_no_overlap(node: &Node, epoch_id: u64) -> Result<(), &'static str> {
    info!("Node {}: Detecting if there is overlap for epoch {}", node.id, epoch_id);

    let mut tracker = node.epoch_round_id.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Overlap for epoch {} detected, but continuing", node.id, epoch_id);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: No overlap detected for epoch {}", node.id, epoch_id);
        Ok(())
    }
}
//TODO: ADD VALIDATION
pub async fn handle_sync_epoch(node: &Node, epoch_id: u64, sender: usize) -> Result<(), &'static str> {
    info!(
        "Node {}: ==== Handling SYNC EPOCH request from Node {} ====",
        node.id, sender
    );

    let mut tracker = node.epoch_round_id.lock().await;
    if tracker.contains(&epoch_id) {
        info!(
            "Node {}: Epoch {} already synchronized with {}",
            node.id, epoch_id, sender
        );
        return Ok(());
    }

    // Update the tracker
    tracker.insert(epoch_id);
    info!(
        "Node {}: Epoch {} synchronized successfully with node {}",
        node.id, epoch_id, sender
    );

    // Persist to TOML configuration
    if let Err(e) = persist_epoch_round_id(epoch_id).await {
        error!(
            "Node {}: Failed to persist Epoch {} to TOML: {:?}",
            node.id, epoch_id, e
        );
        return Err("Failed to persist epoch to TOML.");
    }

    Ok(())
}


pub async fn update_epoch_to_next_round(client: Arc<Client>, path: Option<&str>) {
    // Load the TOML configuration
    let mut toml_config = load_config(path);
    
    // Retrieve the current epoch ID from the TOML configuration
    let mut current_epoch_id = toml_config.consensus.epoch_round_id;

    // Increment the epoch ID
    current_epoch_id += 1;
    info!("Updated epoch to the next round: {}", current_epoch_id);

    // Update the TOML configuration with the new epoch ID
    toml_config.consensus.epoch_round_id = current_epoch_id;

    // Persist the updated TOML configuration back to the file
    if let Err(e) = save_config(&toml_config, path) {
        error!("Failed to save updated epoch ID to TOML file: {:?}", e);
        return;
    }
    info!("Persisted updated epoch ID {} to TOML file.", current_epoch_id);

    // Broadcast the updated epoch to all nodes
    let all_node_urls = toml_config.network.nodes.clone(); // Assume node_urls is a list of all nodes
    for node_url in all_node_urls {
        let sync_url = format!("http://{}/sync_epoch", node_url);
       
       
        let sync_epoch_request = SyncEpochRequest {
            epoch_id: current_epoch_id,
            sender: toml_config.node.id,
        };


        match client.post(&sync_url).json(&sync_epoch_request).send().await {
            Ok(response) if response.status().is_success() => {
                info!(
                    "Successfully synced epoch {} with node at {}",
                    current_epoch_id, node_url
                );
            }
            Ok(response) => {
                error!(
                    "Failed to sync epoch {} with node at {}: HTTP {}",
                    current_epoch_id,
                    node_url,
                    response.status()
                );
            }
            Err(e) => {
                error!(
                    "Error syncing epoch {} with node at {}: {:?}",
                    current_epoch_id, node_url, e
                );
            }
        }
    }
}

