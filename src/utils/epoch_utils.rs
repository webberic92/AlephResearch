use reqwest::Client;
use tracing::info;

use crate::{structs::{node::Node, toml_config::TomlConfig}, utils::dag_utils::check_dag_sync};

pub async fn ensure_no_overlap(node: &Node, epoch_id: u64) -> Result<(), &'static str> {
    info!("Node {}: Detecting if there is overlap for epoch {}", node.id, epoch_id);

    let mut tracker = node.epoch_round_id.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Overlap for epoch {} detected, but continuing", node.id, epoch_id);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: No overlap detected for epoch {}", node.id, epoch_id);
        Ok(())
    }
}

pub async fn handle_sync_epoch(node: &Node, epoch_id: u64, sender: usize) -> Result<(), &'static str> {
    info!(
        "Node {}: ==== Handling SYNC EPOCH request from Node {} ====",
        node.id, sender
    );
    let mut tracker = node.epoch_round_id.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Epoch {} already synchronized with {}", node.id, epoch_id, sender);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: Epoch {} synchronized successfully with node {}", node.id, epoch_id,sender);
        Ok(())
    }
}

pub async fn ensure_epoch_dag_sync(
    client: &Client,
    toml_config: &TomlConfig,
    epoch_id: u64,
) -> Result<(), String> {
    for node_url in &toml_config.network.nodes {
        info!("Checking DAG synchronization with node {} for epoch {}", node_url, epoch_id);
        if let Err(e) = check_dag_sync(client, epoch_id, node_url).await {
            return Err(format!(
                "Epoch DAG synchronization failed with node {} for epoch {}: {:?}",
                node_url, epoch_id, e
            ));
        }
    }
    info!("DAG synchronization successful for epoch {}", epoch_id);
    Ok(())
}