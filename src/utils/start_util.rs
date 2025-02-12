use anyhow::Error;
use reqwest::Client;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use crate::requests::ip_server_requests::is_node_turn;
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



pub async fn wait_for_all_nodes_health(client: &Client, node: Arc<RwLock<Node>>) -> Result<(), Error> {
    let max_retries = 10; // Max attempts
    let mut attempts = 0;

    while attempts < max_retries {
        info!("Checking health of all nodes...");
        let node_read = node.read().await;

        let mut all_healthy = true;

        for node_url in &node_read.nodes {
            let health_url = format!("http://{}/health", node_url);
            match client.get(&health_url).send().await {
                Ok(response) if response.status().is_success() => {
                    info!("Node {} is healthy.", node_url);
                }
                _ => {
                    error!("Node {} health check failed. Retrying...", node_url);
                    all_healthy = false;
                }
            }
        }

        if all_healthy {
            info!("All nodes are healthy.");
            return Ok(());
        }

        attempts += 1;
        sleep(Duration::from_secs(3)).await;
    }

    Err(Error::msg("Timeout waiting for all nodes to become healthy"))
}



pub async fn wait_for_turn(client: &Client, node: Arc<RwLock<Node>>) -> Result<(), Error> {
    loop {
        if is_node_turn(client, node.clone()).await {
            let (node_id, ip_address, latest_epoch) = {
                let node_read = node.read().await;
                let epoch = *node_read.current_round.lock().await; // Fetch latest epoch
                (node_read.id, node_read.ip_address.clone(), epoch)
            };

            info!(
                "Node {} {}: It's my turn to propose for epoch {}",
                node_id, ip_address, latest_epoch
            );

            break;
        }

        sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

