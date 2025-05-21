use anyhow::Error;
use reqwest::Client;
use tokio::sync::Mutex;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
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
        match client.get(&url).timeout(Duration::from_millis(500)).send().await {
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
pub async fn wait_for_all_nodes_health( node: Arc<Mutex<Node>>) -> Result<(), Error> {
    loop {
        info!("🔍 Checking health of all nodes...");

        let nodes = {
            let node_guard = node.lock().await;
            node_guard.nodes.clone()
        };

        let local_client = reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
        .build()
        .expect("Failed to build HTTP client");
        let mut unhealthy_nodes = Vec::new();

        let mut health_checks = vec![];
        for node_url in nodes {
            let client = local_client.clone();
            let url = node_url.clone();

            let check = async move {
                let health_url = format!("http://{}/health", url);
                let success = match client.get(&health_url).send().await {
                    Ok(response) => response.status().is_success(),
                    Err(_) => false,
                };

                if !success {
                    Some(url)
                } else {
                    None
                }
            };

            health_checks.push(check);
        }

        let results: Vec<Option<String>> = futures::future::join_all(health_checks).await;
        for result in results.into_iter().flatten() {
            unhealthy_nodes.push(result);
        }

        if unhealthy_nodes.is_empty() {
            info!("✅ All nodes are healthy.");
            return Ok(());
        } else {
            warn!("⛔ Unhealthy nodes detected: {:?}: Trying again....", unhealthy_nodes);
            sleep(Duration::from_secs(1)).await;
        }
    }
}



