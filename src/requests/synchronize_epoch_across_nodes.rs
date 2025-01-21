use reqwest::Client;
use tracing::{error, info};
use serde_json::json;
use crate::structs::toml_config::TomlConfig;

/// Ensure epoch synchronization across nodes
pub async fn synchronize_epoch_across_nodes(client: &Client, toml_config: &TomlConfig) -> bool {
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