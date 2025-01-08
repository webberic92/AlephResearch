use reqwest::Client;
use tracing::{error, info};
use serde_json::json;

use crate::structs::toml_config::TomlConfig;

/// Check if it's this node's turn to submit a transaction
pub async fn is_node_turn(client: &Client, toml_config: &TomlConfig, current_epoch: u64) -> bool {
    let url = format!(
        "http://{}:8080/is_turn?node_id={}&epoch_id={}",
        toml_config.network.ip_manager_address, toml_config.node.id, current_epoch
    );
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let body: serde_json::Value = response.json().await.unwrap();
                body["is_turn"].as_bool().unwrap_or(false)
            } else {
                false
            }
        }
        Err(e) => {
            error!("Error checking turn for node {}: {:?}", toml_config.node.id, e);
            false
        }
    }
}

pub async fn notify_transaction_submitted(client: &Client, toml_config: &TomlConfig, node_id: usize) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("http://{}:8080/submit_transaction", toml_config.network.ip_manager_address);
    let payload = json!({ "node_id": node_id });
    
    let response = client.post(&url).json(&payload).send().await?;
    if response.status().is_success() {
        info!("Node {}: Successfully notified python server transaction submission.", node_id);
    } else {
        error!("Node {}: Failed to notify python server transaction submission. Status: {}", node_id, response.status());
    }
    Ok(())
}