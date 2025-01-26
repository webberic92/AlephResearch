use std::sync::Arc;

use reqwest::Client;
use tracing::{error, info};

use crate::{structs::{node::Node, toml_config::TomlConfig}, utils::{config_util::load_config, dag_utils::check_dag_sync}};

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

pub async fn handle_sync_epoch(node: &Node, epoch_id: u64, sender: usize) -> Result<(), &'static str> {
    info!(
        "Node {}: ==== Handling SYNC EPOCH request from Node {} ====",
        node.id, sender
    );
    let mut tracker = node.epoch_round_id.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Epoch {} already synchronized with {}", node.id, epoch_id, sender);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: Epoch {} synchronized successfully with node {}", node.id, epoch_id,sender);
        Ok(())
    }
}

// pub async fn ensure_epoch_dag_sync(
//     client: &Client,
//     toml_config: &TomlConfig,
//     epoch_id: u64,
// ) -> Result<(), String> {
//     for node_url in &toml_config.network.nodes {
//         info!("Checking DAG synchronization with node {} for epoch {}", node_url, epoch_id);
//         if let Err(e) = check_dag_sync(client, epoch_id, node_url).await {
//             return Err(format!(
//                 "Epoch DAG synchronization failed with node {} for epoch {}: {:?}",
//                 node_url, epoch_id, e
//             ));
//         }
//     }
//     info!("DAG synchronization successful for epoch {}", epoch_id);
//     Ok(())
// }

pub async fn update_epoch_to_next_round(client: &Arc<Client>) {
    // Retrieve the current epoch ID from the TOML configuration
    let toml_config = load_config();
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