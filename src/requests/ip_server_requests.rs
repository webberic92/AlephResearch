use reqwest::Client;
use tracing::{error, info, warn};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::structs::node::Node;

/// Check if it's this node's turn to submit a transaction
pub async fn is_node_turn(client: &Client, node: Arc<RwLock<Node>>) -> bool {
    let (node_id, ip_manager_address, current_round) = {
        let node_read = node.read().await;
        let round = *node_read.current_round.lock().await;
        (node_read.id, node_read.ip_manager_address.clone(), round)
    }; // 🔴 Drop read lock immediately

    let url = format!(
        "http://{}:8080/is_turn?node_id={}&round_id={}",
        ip_manager_address, node_id, current_round
    );

    info!(
        "Node {}: Checking if it's the turn to propose for round {} at URL: {}",
        node_id, current_round, url
    );

    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => response.json::<serde_json::Value>().await
            .map(|body| body["is_turn"].as_bool().unwrap_or(false))
            .unwrap_or(false),
        Ok(response) => {
            let status = response.status(); // Get HTTP status
            let text = response.text().await.unwrap_or_else(|_| "Failed to parse response".to_string());
        
            warn!(
                "Node {}: Turn check failed with status {} for round {}. Response: {}",
                node_id, status, current_round, text
            );
            false
        },
        Err(e) => {
            error!(
                "Node {}: Error while checking turn for round {}: {:?}",
                node_id, current_round, e
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

