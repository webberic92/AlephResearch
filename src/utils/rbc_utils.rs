use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing::error;
use serde_json::json;
use crate::structs::toml_config::TomlConfig;


/// Ensure epoch synchronization across nodes
pub async fn ensure_epoch_sync(client: &Client, toml_config: &TomlConfig) -> bool {
    for node_url in &toml_config.network.nodes {
        let url = format!("http://{}/sync_epoch", node_url);
        let payload = json!({ "epoch_id": toml_config.consensus.epoch_round_id, "sender": &toml_config.node.id });
        if let Err(e) = client.post(&url).json(&payload).send().await {
            error!("Failed to synchronize epoch with node {}: {:?}", node_url, e);
            return false;
        }
    }
    info!("Epoch {} synchronized across all nodes.", toml_config.consensus.epoch_round_id);
    true
}

// Helper functions
pub async fn check_all_nodes_health(client: &Client, toml_config: &TomlConfig) -> bool {
    for node in &toml_config.network.nodes {
        let url = format!("http://{}/health", node);
        match client.get(&url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    info!("Node {} is not healthy. Retrying...", node);
                    return false;
                }
            }
            Err(e) => {
                info!("Node {} health check failed with error: {:?}", node, e);
                return false;
            }
        }
    }
    true
}

pub async fn wait_for_all_nodes_health(client: &Client, toml_config: &TomlConfig) {
    loop {
        info!("Checking health of all nodes...");
        if check_all_nodes_health(client, toml_config).await {
            info!("All nodes are healthy!");
            break;
        }
        info!("Some nodes are not healthy. Retrying in 1 second...");
        sleep(Duration::from_secs(1)).await;
    }
}