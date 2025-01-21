use reqwest::Client;
use tracing::{error, info, warn};
use serde_json::json;

use crate::structs::toml_config::TomlConfig;

/// Check if it's this node's turn to submit a transaction
pub async fn is_node_turn(client: &Client, toml_config: &TomlConfig, current_epoch: u64) -> bool {
    let url = format!(
        "http://{}:8080/is_turn?node_id={}&epoch_id={}",
        toml_config.network.ip_manager_address, toml_config.node.id, current_epoch
    );
    
    info!(
        "Node {}: Checking if it's the turn for node to propose for epoch {} at URL: {}",
        toml_config.node.id, current_epoch, url
    );

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let is_turn = body["is_turn"].as_bool().unwrap_or(false);
                        if is_turn {
                            info!(
                                "Node {}: It is the turn for this node to propose for epoch {}",
                                toml_config.node.id, current_epoch
                            );
                        } else {
                            info!(
                                "Node {}: It is NOT the turn for this node to propose for epoch {}",
                                toml_config.node.id, current_epoch
                            );
                        }
                        is_turn
                    }
                    Err(e) => {
                        error!(
                            "Node {}: Failed to parse response JSON while checking turn for epoch {}: {:?}",
                            toml_config.node.id, current_epoch, e
                        );
                        false
                    }
                }
            } else if status.as_u16() == 403 {
                match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let error_message = body["error"].as_str().unwrap_or("Unknown error");
                        let expected_node_id = body["expected_node_id"].as_u64().unwrap_or(0);
                        let expected_epoch_id = body["expected_epoch_id"].as_u64().unwrap_or(0);
                        let received_node_id = body["received_node_id"].as_u64().unwrap_or(0);
                        let received_epoch_id = body["received_epoch_id"].as_u64().unwrap_or(0);

                        warn!(
                            "Node {}: Turn check failed with status 403. Details: Error = {}, \
                             Expected Node ID = {}, Expected Epoch ID = {}, Received Node ID = {}, \
                             Received Epoch ID = {}",
                            toml_config.node.id, error_message, expected_node_id,
                            expected_epoch_id, received_node_id, received_epoch_id
                        );
                    }
                    Err(e) => {
                        error!(
                            "Node {}: Failed to parse 403 response JSON while checking turn for epoch {}: {:?}",
                            toml_config.node.id, current_epoch, e
                        );
                    }
                }
                false
            } else {
                warn!(
                    "Node {}: Received non-success status {} while checking turn for epoch {}",
                    toml_config.node.id, status, current_epoch
                );
                false
            }
        }
        Err(e) => {
            error!(
                "Node {}: Error occurred while making request to check turn for epoch {}: {:?}",
                toml_config.node.id, current_epoch, e
            );
            false
        }
    }
}


pub async fn notify_transaction_submitted(client: &Client, toml_config: &TomlConfig) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("http://{}:8080/submit_transaction", toml_config.network.ip_manager_address);
    let payload = json!({ "node_id": toml_config.node.id });
    
    let response = client.post(&url).json(&payload).send().await?;
    if response.status().is_success() {
        info!("Node {}: Successfully notified python server transaction submission.", toml_config.node.id);
    } else {
        error!("Node {}: Failed to notify python server transaction submission. Status: {}", toml_config.node.id, response.status());
    }
    Ok(())
}