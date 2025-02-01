use reqwest::Client;
use tracing::{error, info};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::structs::node::Node;

/// Synchronize the current epoch across all nodes
pub async fn synchronize_epoch_across_nodes(client: &Client, node: Arc<RwLock<Node>>) -> bool {
    let node_read = node.read().await;
    let node_id = node_read.id;
    let current_epoch = {
        let epoch_guard = node_read.current_epoch.lock().await;
        *epoch_guard
    };

    for node_url in &node_read.nodes {
        let url = format!("http://{}/sync_epoch", node_url);
        let payload = json!({ "epoch_id": current_epoch, "sender": node_id });

        if let Err(e) = client.post(&url).json(&payload).send().await {
            error!("Failed to synchronize epoch with node {}: {:?}", node_url, e);
            return false;
        }
    }

    info!("Epoch {} synchronized across all nodes.", current_epoch);
    true
}
