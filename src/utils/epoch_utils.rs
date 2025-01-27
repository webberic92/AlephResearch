use std::sync::Arc;

use tracing::{error, info};

use crate::{structs::{node::Node, toml_config::TomlConfig}, utils::{config_util::{load_config, persist_epoch_round_id}}};

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


//THIS IS ONLY IN TESTS RN
pub async fn update_epoch_to_next_round(client: &Arc<reqwest::Client>, path: Option<&str>) {
    let toml_config = load_config(path);
    // Retrieve the current epoch ID from the TOML configuration
    // let toml_config = load_config(None);
    let mut current_epoch_id = toml_config.consensus.epoch_round_id;

    // Increment the epoch ID
    current_epoch_id += 1;
    info!("Updated epoch to the next round: {}", current_epoch_id);

    // Broadcast the updated epoch to all nodes
    let all_node_urls = toml_config.network.nodes.clone(); // Assume node_urls is a list of all nodes
    for node_url in all_node_urls {
        let sync_url = format!("{}/sync_epoch", node_url);
        let payload = serde_json::json!({
            "epoch_id": current_epoch_id,
            "sender": toml_config.node.id, // Assuming node_id is in the TOML config
        });

        match client.post(&sync_url).json(&payload).send().await {
            Ok(response) if response.status().is_success() => {
                info!("Successfully synced epoch {} with node at {}", current_epoch_id, node_url);
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

// pub async fn update_proposal_tracker(node: &Node, sender_id: usize, epoch_id: u64) {
//     let mut tracker = node.proposal_tracker.lock().await;

//     // Add the proposal if it doesn't already exist
//     tracker.entry(epoch_id).or_insert_with(Vec::new).push(sender_id); // Arc<Mutex<HashSet<usize>>>,

//     // Sort proposals by epoch ID to ensure prioritization
//     tracker.sort_by_key(|&(epoch, _)| epoch);
// }