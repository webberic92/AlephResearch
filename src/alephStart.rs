use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::{fs, sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{error, info};
use tracing_subscriber;

// Configuration structures
#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    consensus: ConsensusConfig,
    node: NodeConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    listen_address: String,
    nodes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConsensusConfig {
    transaction_size: usize,
    data_shards: usize,
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    id: usize,
    total_nodes: usize,
}

// Data structure with epoch ID
#[derive(Debug, Clone)]
struct Data {
    transaction: Vec<u8>,
    epoch_id: u64,
}

// Node structure
#[derive(Debug, Clone)]
struct Node {
    id: usize,
    total_nodes: usize,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
}

impl Node {
    fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

// Helper functions

/// Compute Merkle root from shard hashes
fn compute_merkle_root(hashes: &[Vec<u8>]) -> Vec<u8> {
    if hashes.len() == 1 {
        return hashes[0].clone();
    }
    let mut next_level = vec![];
    for pair in hashes.chunks(2) {
        let mut combined = pair[0].clone();
        if pair.len() > 1 {
            combined.extend(&pair[1]);
        }
        next_level.push(Sha256::digest(&combined).to_vec());
    }
    compute_merkle_root(&next_level)
}


/// Check the health of all nodes
async fn check_all_nodes_health(client: &Client, nodes: &[String]) -> bool {
    for node in nodes {
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

/// Wait until all nodes are healthy
async fn wait_for_all_nodes_health(client: &Client, nodes: &[String]) {
    loop {
        info!("Checking health of all nodes...");
        if check_all_nodes_health(client, nodes).await {
            info!("All nodes are healthy!");
            break;
        }
        info!("Some nodes are not healthy. Retrying in 5 seconds...");
        sleep(Duration::from_secs(5)).await;
    }
}

/// Synchronize epoch states
async fn synchronize_epoch_states(
    nodes: &[String],
    client: &Client,
    epoch_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    for node_url in nodes {
        let url = format!("http://{}/sync_epoch", node_url);
        let response = client
            .post(&url)
            .json(&json!({ "epoch_id": epoch_id }))
            .send()
            .await;

        match response {
            Ok(res) => {
                if !res.status().is_success() {
                    return Err(format!(
                        "Failed to synchronize epoch {} with node {}: {}",
                        epoch_id, node_url, res.status()
                    )
                    .into());
                }
            }
            Err(e) => {
                return Err(format!(
                    "Error synchronizing epoch {} with node {}: {:?}",
                    epoch_id, node_url, e
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Load configuration
fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();
    let current_epoch = 1;

    let _node = Arc::new(Node::new(config.node.id, config.node.total_nodes));

    // Ensure all nodes are healthy
    wait_for_all_nodes_health(&client, &config.network.nodes).await;

    // Synchronize epoch states before starting proposals
    if let Err(e) = synchronize_epoch_states(&config.network.nodes, &client, current_epoch).await {
        error!("Failed to synchronize epoch {}: {}", current_epoch, e);
        return Err(e);
    }
    info!("Epoch {} synchronized successfully", current_epoch);

    Ok(())
}
