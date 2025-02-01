use reqwest::Client;
use tracing::{error, info, warn};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::structs::node::Node;

/// Check if it's this node's turn to submit a transaction
pub async fn is_node_turn(client: &Client, node: Arc<RwLock<Node>>) -> bool {
    let node_read = node.read().await;
    let node_id = node_read.id;
    let ip_manager_address = &node_read.ip_manager_address;
    let current_epoch = {
        let epoch_guard = node_read.current_epoch.lock().await;
        *epoch_guard
    };

    let url = format!(
        "http://{}:8080/is_turn?node_id={}&epoch_id={}",
        ip_manager_address, node_id, current_epoch
    );

    info!(
        "Node {}: Checking if it's the turn to propose for epoch {} at URL: {}",
        node_id, current_epoch, url
    );

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let is_turn = body["is_turn"].as_bool().unwrap_or(false);
                        info!(
                            "Node {}: It {} the turn to propose for epoch {}",
                            node_id,
                            if is_turn { "IS" } else { "is NOT" },
                            current_epoch
                        );
                        is_turn
                    }
                    Err(e) => {
                        error!(
                            "Node {}: Failed to parse response JSON while checking turn for epoch {}: {:?}",
                            node_id, current_epoch, e
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
                            node_id, error_message, expected_node_id,
                            expected_epoch_id, received_node_id, received_epoch_id
                        );
                    }
                    Err(e) => {
                        error!(
                            "Node {}: Failed to parse 403 response JSON while checking turn for epoch {}: {:?}",
                            node_id, current_epoch, e
                        );
                    }
                }
                false
            } else {
                warn!(
                    "Node {}: Received non-success status {} while checking turn for epoch {}",
                    node_id, status, current_epoch
                );
                false
            }
        }
        Err(e) => {
            error!(
                "Node {}: Error occurred while making request to check turn for epoch {}: {:?}",
                node_id, current_epoch, e
            );
            false
        }
    }
}

pub async fn notify_transaction_submitted(
    client: &Client,
    node: Arc<RwLock<Node>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let node_read = node.read().await;
    let node_id = node_read.id;
    let ip_manager_address = &node_read.ip_manager_address;

    let url = format!("http://{}:8080/submit_transaction", ip_manager_address);
    let payload = json!({ "node_id": node_id });

    let response = client.post(&url).json(&payload).send().await?;
    if response.status().is_success() {
        info!(
            "Node {}: Successfully notified Python server of transaction submission.",
            node_id
        );
    } else {
        error!(
            "Node {}: Failed to notify Python server of transaction submission. Status: {}",
            node_id,
            response.status()
        );
    }
    Ok(())
}

