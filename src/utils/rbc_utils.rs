use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use crate::requests::ip_server_requests::is_node_turn;
use crate::requests::synchronize_epoch_across_nodes::synchronize_epoch_across_nodes;
use crate::structs::toml_config::TomlConfig;

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

pub async fn wait_for_turn(
    client: &Client,
    toml_config: &TomlConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Waiting for its turn to submit transaction for epoch {}",
        toml_config.node.id, toml_config.network.ip_address, toml_config.consensus.epoch_round_id
    );

    loop {
        if is_node_turn(client, toml_config, toml_config.consensus.epoch_round_id).await {
            break;
        }

        synchronize_epoch_across_nodes(client, toml_config).await;
        sleep(Duration::from_secs(1)).await; // Poll every 1 second
        info!(
            "Node {} {}: Retrying transaction submission for epoch {}",
            toml_config.node.id, toml_config.network.ip_address, toml_config.consensus.epoch_round_id
        );
    }

    info!(
        "Node {} {}: It's my turn to propose for epoch {}",
        toml_config.node.id, toml_config.network.ip_address, toml_config.consensus.epoch_round_id
    );
    Ok(())
}