use std::sync::Arc;

use reqwest::Client;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::structs::{node::Node, requests::SyncEpochRequest};

/// Updates the epoch in the local node state.
pub async fn update_local_epoch(node: Arc<RwLock<Node>>) -> u64 {
    let next_epoch_id: u64;

    {
        // Acquire a write lock to update the epoch ID
        let node_state = node.write().await;
        let mut current_epoch = node_state.current_epoch.lock().await;

        // Increment the epoch ID
        next_epoch_id = *current_epoch + 1;
        *current_epoch = next_epoch_id;

        info!(
            "Node {}: Updated local epoch to the next round: {}",
            node_state.id, next_epoch_id
        );
    }

    next_epoch_id
}


/// Broadcasts the updated epoch to all nodes.
pub async fn broadcast_epoch_update(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    epoch_id: u64,
) -> Result<(), String> {
    let node_urls = {
        let node_state = node.read().await;
        node_state.nodes.clone()
    };

    for node_url in node_urls {
        let sync_url = format!("http://{}/sync_epoch", node_url); // ✅ Fixed double http:// issue

        let sync_epoch_request = SyncEpochRequest {
            epoch_id,
            sender: node.read().await.id,
        };

        match client.post(&sync_url).json(&sync_epoch_request).send().await {
            Ok(response) if response.status().is_success() => {
                info!(
                    "Node {}: Successfully synced epoch {} with node at {}",
                    node.read().await.id, epoch_id, node_url
                );
            }
            Ok(response) => {
                let error_message = format!(
                    "Node {}: Failed to sync epoch {} with node at {}: HTTP {}",
                    node.read().await.id, epoch_id, node_url, response.status()
                );
                error!("{}", error_message);
                return Err(error_message);
            }
            Err(e) => {
                let error_message = format!(
                    "Node {}: Error syncing epoch {} with node at {}: {:?}",
                    node.read().await.id, epoch_id, node_url, e
                );
                error!("{}", error_message);
                return Err(error_message);
            }
        }
    }

    Ok(())
}

