use anyhow::Error;
use reqwest::Client;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use crate::requests::ip_server_requests::is_node_turn;
use crate::structs::node::Node;

/// **🔄 Updated: Use `Arc<Mutex<Node>>`**  
/// - Ensures consistency with the new thread-safe architecture.
pub async fn check_all_nodes_health(client: &Client, node: Arc<Mutex<Node>>) -> bool {
    let nodes = {
        //info!("🔍 [DEBUG] Waiting to acquire node lock for start utils");
let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for start utils");
        node_guard.nodes.clone()
    }; // 🔓 Lock released immediately here

    let mut all_healthy = true;

    for peer in nodes {
        let url = format!("http://{}/health", peer);
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                info!("Node {} is healthy.", peer);
            }
            Ok(response) => {
                info!("Node {} responded with status {}. Retrying...", peer, response.status());
                all_healthy = false;
            }
            Err(e) => {
                info!("Node {} health check failed with error: {:?}", peer, e);
                all_healthy = false;
            }
        }
    }

    all_healthy
}


/// **🛠️ Wait until all nodes report healthy**  
/// - Retries up to 10 times, checking every 3 seconds.
pub async fn wait_for_all_nodes_health(client: &Client, node: Arc<Mutex<Node>>) -> Result<(), Error> {
    let max_retries = 10;
    let mut attempts = 0;

    while attempts < max_retries {
        info!("Checking health of all nodes...");

        let nodes = {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for start utils");
let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for start utils");
            node_guard.nodes.clone()
        };

        let health_checks = nodes.into_iter().map(|node_url| {
            let client = client.clone();
            async move {
                let health_url = format!("http://{}/health", node_url);
                match client.get(&health_url).send().await {
                    Ok(response) => response.status().is_success(),
                    Err(_) => false,
                }
            }
        });

        let results: Vec<bool> = futures::future::join_all(health_checks).await;
        if results.iter().all(|&r| r) {
            info!("All nodes are healthy.");
            return Ok(());
        }

        attempts += 1;
        sleep(Duration::from_secs(3)).await;
    }

    Err(Error::msg("Timeout waiting for all nodes to become healthy"))
}


/// **🕰️ Wait for Node's Turn**  
/// - Continuously checks if it is this node's turn.
pub async fn wait_for_turn(client: &Client, node: Arc<Mutex<Node>>) -> Result<(), Error> {
    loop {
        if is_node_turn(client, node.clone()).await {
            let (node_id, ip_address, latest_round) = {
                //info!("🔍 [DEBUG] Waiting to acquire node lock for start utils");
let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for start utils");
                let round = *node_guard.current_round.lock().await; 
                (node_guard.id, node_guard.ip_address.clone(), round)
            };

            info!(
                "Node {} {}: It's my turn to propose for round {}",
                node_id, ip_address, latest_round
            );

            break;
        }

        sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}
