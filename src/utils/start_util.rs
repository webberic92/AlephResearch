use reqwest::Client;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use crate::requests::ip_server_requests::is_node_turn;
use crate::requests::synchronize_epoch_across_nodes::synchronize_epoch_across_nodes;
use crate::structs::node::Node;

// Helper functions
pub async fn check_all_nodes_health(client: &Client, node: Arc<RwLock<Node>>) -> bool {
    let node_read = node.read().await;
    let mut all_healthy = true;

    for peer in &node_read.nodes {
        let url = format!("http://{}/health", peer);
        match client.get(&url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    info!("Node {} is not healthy. Retrying...", peer);
                    all_healthy = false; // Mark as unhealthy but continue checking
                }
            }
            Err(e) => {
                info!("Node {} health check failed with error: {:?}", peer, e);
                all_healthy = false; // Mark as unhealthy but continue checking
            }
        }
    }

    all_healthy
}



pub async fn wait_for_all_nodes_health(client: &Client, node: Arc<RwLock<Node>>) {
    loop {
        info!("Checking health of all nodes...");
        let all_healthy = check_all_nodes_health(client, node.clone()).await;

        if all_healthy {
            info!("All nodes are healthy!");
            break;
        }

        info!("Some nodes are not healthy. Retrying in 1 second...");
        sleep(Duration::from_secs(1)).await;
    }
}



pub async fn wait_for_turn(
    client: &Client,
    node: Arc<RwLock<Node>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let node_read = node.read().await;
    let node_id = node_read.id;
    let ip_address = &node_read.ip_address;

    let current_epoch = {
        let epoch_guard = node_read.current_epoch.lock().await;
        *epoch_guard // Extract the value
    };

    info!(
        "Node {} {}: Waiting for its turn to submit transaction for epoch {}",
        node_id, ip_address, current_epoch
    );

    loop {
        if is_node_turn(client, node.clone()).await {
            break;
        }

        synchronize_epoch_across_nodes(client, node.clone()).await;
        sleep(Duration::from_secs(1)).await;

        info!(
            "Node {} {}: Retrying transaction submission for epoch {}",
            node_id, ip_address, current_epoch
        );
    }

    info!(
        "Node {} {}: It's my turn to propose for epoch {}",
        node_id, ip_address, current_epoch
    );
    Ok(())
}