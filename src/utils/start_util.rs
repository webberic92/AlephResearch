use anyhow::Error;
use reqwest::Client;
use tokio::sync::Mutex;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
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
        let message_count = node.lock().await.message_count.clone();
        message_count.fetch_add(1, Ordering::Relaxed);
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
    let mut attempts = 1;
    info!("Node: Entering wait_for_all_nodes_health.");

    while attempts < max_retries {
        info!("Checking health of all nodes...attempt {}", attempts);

        let nodes = {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for start utils");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for start utils");
            node_guard.nodes.clone()
        };

        let message_count = node.lock().await.message_count.clone(); // ✅ Clone before the async block

        let health_checks = nodes.into_iter().map(|node_url| {
            let client = client.clone();
            let message_count = message_count.clone(); // ✅ Clone again for each async task
        
            async move {
                let health_url = format!("http://{}/health", node_url);
                let success = match client.get(&health_url).send().await {
                    Ok(response) => response.status().is_success(),
                    Err(_) => false,
                };
        
                message_count.fetch_add(1, Ordering::Relaxed); // ✅ Always increment after request
        
                success
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



