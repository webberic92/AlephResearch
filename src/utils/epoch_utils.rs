use std::sync::Arc;

use reqwest::Client;
use tokio::sync::RwLock;
use tracing::{error, info};
use tokio::time::timeout;
use std::time::Duration;
use crate::structs::{node::Node, requests::SyncEpochRequest};

/// Updates the epoch in the local node state.
pub async fn update_local_epoch(node: Arc<RwLock<Node>>) -> u64 {
    // info!("Entering Update local epoch to the next round");

    let next_epoch_id;
    
    // ✅ Step 1: Acquire a read lock on `node` to get `current_epoch`
    let current_epoch_lock = {
        let node_read = node.read().await; // Keeps `node_read` in scope
        node_read.current_epoch.clone()    // Clone the Arc<Mutex<u64>> to extend its lifetime
    };

    // ✅ Step 2: Lock `current_epoch` separately
    {
        let mut epoch_guard = current_epoch_lock.lock().await;
        next_epoch_id = *epoch_guard + 1;
        *epoch_guard = next_epoch_id;
    } // 🔴 Drop `current_epoch` lock immediately

    // ✅ Step 3: Read `node.id` separately
    let node_id = {
        let node_read = node.read().await;
        node_read.id
    }; // 🔴 Drop `node` read lock immediately

    info!(
        "Node {}: Updated local epoch to the next round: {}",
        node_id, next_epoch_id
    );

    // info!("Exiting Update local epoch to the next round");

    next_epoch_id
}



/// Broadcasts the updated epoch to all nodes.
pub async fn broadcast_epoch_update(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    epoch_id: u64,
) -> Result<(), String> {
    info!("Node {}: Entering broadcast_epoch_update for epoch {}", node.read().await.id, epoch_id);

    let node_urls = {
        let node_state = node.read().await;
        node_state.nodes.clone()
    };

    info!(
        "Node {}: About to send sync_epoch requests to all nodes: {:?}",
        node.read().await.id, node_urls
    );

    let mut failed_syncs = Vec::new(); // Track failures

    for node_url in node_urls {
        let sync_url = format!("http://{}/sync_epoch", node_url);

        let sync_epoch_request = SyncEpochRequest {
            epoch_id,
            sender: node.read().await.id,
        };
        info!(
            "Node {}: Broadcasting epoch update for epoch {} to node {}",
            node.read().await.id, epoch_id, node_url
        );
        match timeout(Duration::from_secs(5), client.post(&sync_url).json(&sync_epoch_request).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                info!(
                    "Node {}: Successfully synced epoch {} with node {}",
                    node.read().await.id, epoch_id, node_url
                );
            }
            Ok(Ok(response)) => {
                let error_message = format!(
                    "Node {}: Failed to sync epoch {} with node {}. HTTP {}",
                    node.read().await.id, epoch_id, node_url, response.status()
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Ok(Err(e)) => {
                let error_message = format!(
                    "Node {}: Error syncing epoch {} with node {}: {:?}",
                    node.read().await.id, epoch_id, node_url, e
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Err(_) => {
                let error_message = format!(
                    "Node {}: Timeout while syncing epoch {} with node {}",
                    node.read().await.id, epoch_id, node_url
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
        }
    }

    if !failed_syncs.is_empty() {
        error!(
            "Node {}: Some epoch syncs failed: {:?}",
            node.read().await.id, failed_syncs
        );
        // Don't return an error to avoid blocking progress
    }

    info!("Node {}: Finished broadcasting epoch update for epoch {}", node.read().await.id, epoch_id);

    Ok(())
}


