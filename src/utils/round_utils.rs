use std::sync::Arc;

use reqwest::Client;
use tokio::sync::RwLock;
use tracing::{error, info};
use tokio::time::timeout;
use std::time::Duration;
use crate::structs::{node::Node, requests::SyncroundRequest};

/// Updates the round in the local node state.
pub async fn update_local_round(node: Arc<RwLock<Node>>) -> u64 {
    // info!("Entering Update local round to the next round");

    let next_round_id;
    
    // ✅ Step 1: Acquire a read lock on `node` to get `current_round`
    let current_round_lock = {
        let node_read = node.read().await; // Keeps `node_read` in scope
        node_read.current_round.clone()    // Clone the Arc<Mutex<u64>> to extend its lifetime
    };

    // ✅ Step 2: Lock `current_round` separately
    {
        let mut round_guard = current_round_lock.lock().await;
        next_round_id = *round_guard + 1;
        *round_guard = next_round_id;
    } // 🔴 Drop `current_round` lock immediately

    // ✅ Step 3: Read `node.id` separately
    let node_id = {
        let node_read = node.read().await;
        node_read.id
    }; // 🔴 Drop `node` read lock immediately

    info!(
        "Node {}: Updated local round to the next round: {}",
        node_id, next_round_id
    );

    // info!("Exiting Update local round to the next round");

    next_round_id
}



/// Broadcasts the updated round to all nodes.
pub async fn broadcast_round_update(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    round_id: u64,
) -> Result<(), String> {
    info!("Node {}: Entering broadcast_round_update for round {}", node.read().await.id, round_id);

    let node_urls = {
        let node_state = node.read().await;
        node_state.nodes.clone()
    };

    info!(
        "Node {}: About to send sync_round requests to all nodes: {:?}",
        node.read().await.id, node_urls
    );

    let mut failed_syncs = Vec::new(); // Track failures

    for node_url in node_urls {
        let sync_url = format!("http://{}/sync_round", node_url);

        let sync_round_request = SyncroundRequest {
            round_id,
            sender: node.read().await.id,
        };
        info!(
            "Node {}: Broadcasting round update for round {} to node {}",
            node.read().await.id, round_id, node_url
        );
        match timeout(Duration::from_secs(5), client.post(&sync_url).json(&sync_round_request).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                info!(
                    "Node {}: Successfully synced round {} with node {}",
                    node.read().await.id, round_id, node_url
                );
            }
            Ok(Ok(response)) => {
                let error_message = format!(
                    "Node {}: Failed to sync round {} with node {}. HTTP {}",
                    node.read().await.id, round_id, node_url, response.status()
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Ok(Err(e)) => {
                let error_message = format!(
                    "Node {}: Error syncing round {} with node {}: {:?}",
                    node.read().await.id, round_id, node_url, e
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Err(_) => {
                let error_message = format!(
                    "Node {}: Timeout while syncing round {} with node {}",
                    node.read().await.id, round_id, node_url
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
        }
    }

    if !failed_syncs.is_empty() {
        error!(
            "Node {}: Some round syncs failed: {:?}",
            node.read().await.id, failed_syncs
        );
        // Don't return an error to avoid blocking progress
    }

    info!("Node {}: Finished broadcasting round update for round {}", node.read().await.id, round_id);

    Ok(())
}


