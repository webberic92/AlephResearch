use serde_json::json;
use tracing::{error, info};
use reqwest::Client;
use crate::structs::node::Node;

/// Checks whether the local DAG is synchronized with the target node's DAG.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `client`: HTTP client for making requests.
/// - `epoch_id`: The epoch ID for which to check synchronization.
/// - `target_node_id`: The ID of the target node.
///
/// # Returns
/// - `Result<bool, Box<dyn std::error::Error>>`: `Ok(true)` if synchronized, `Ok(false)` if not, or an error.
pub async fn check_dag_sync(
    node: &Node,
    client: &Client,
    epoch_id: u64,
    target_node_url: &String,
) -> Result<bool, Box<dyn std::error::Error>> {
    let target_node_url = format!("http://{}/dag_status", target_node_url); // Replace with actual URL

    let payload = json!({
        "epoch_id": epoch_id,
        "node_id": node.id,
    });

    let response = client.post(&target_node_url).json(&payload).send().await?;

    if response.status().is_success() {
        let response_body = response.json::<serde_json::Value>().await?;
        let in_sync = response_body["in_sync"].as_bool().unwrap_or(false);
        Ok(in_sync)
    } else {
        error!(
            "Node {}: Failed to fetch DAG sync status from Node {} for epoch {}. Status: {}",
            node.id, target_node_url, epoch_id, response.status()
        );
        Ok(false)
    }
}
