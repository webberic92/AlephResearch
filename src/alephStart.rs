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
#[derive(Debug, Deserialize, serde::Serialize)]
struct Config {
    network: NetworkConfig,
    consensus: ConsensusConfig,
    node: NodeConfig,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct NetworkConfig {
    listen_address: String,
    nodes: Vec<String>,
    ip_manager_address: String, // Added for GTC APIs
    proposals: Vec<usize>, // Add this line
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct ConsensusConfig {
    transaction_size: usize,
    data_shards: usize,
    batch_size: usize,
}

#[derive(Debug, Deserialize, serde::Serialize)]
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

async fn wait_for_all_nodes_health(client: &Client, nodes: &[String]) {
    loop {
        info!("Checking health of all nodes...");
        if check_all_nodes_health(client, nodes).await {
            info!("All nodes are healthy!");
            break;
        }
        info!("Some nodes are not healthy. Retrying in 1 second...");
        sleep(Duration::from_secs(1)).await;
    }
}

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

/// Compute the Merkle branch for a given index in the Merkle tree
fn compute_merkle_branch(hashes: &[Vec<u8>], index: usize) -> Vec<Vec<u8>> {
    let mut branch = vec![];
    let mut current_index = index;
    let mut current_level = hashes.to_vec();

    while current_level.len() > 1 {
        let sibling_index = if current_index % 2 == 0 {
            current_index + 1
        } else {
            current_index - 1
        };

        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
        }

        current_index /= 2;
        current_level = current_level
            .chunks(2)
            .map(|pair| {
                let mut combined = pair[0].clone();
                if pair.len() > 1 {
                    combined.extend(&pair[1]);
                }
                Sha256::digest(&combined).to_vec()
            })
            .collect();
    }

    branch
}

/// Ensure epoch synchronization across nodes
async fn ensure_epoch_sync(client: &Client, config: &Config, current_epoch: u64) -> bool {
    for node_url in &config.network.nodes {
        let url = format!("http://{}/sync_epoch", node_url);
        let payload = json!({ "epoch_id": current_epoch, "sender": &config.node.id });
        if let Err(e) = client.post(&url).json(&payload).send().await {
            error!("Failed to synchronize epoch with node {}: {:?}", node_url, e);
            return false;
        }
    }
    info!("Epoch {} synchronized across all nodes.", current_epoch);
    true
}

/// Check if it's this node's turn to submit a transaction
async fn is_node_turn(client: &Client, config: &Config, current_epoch: u64) -> bool {
    let url = format!(
        "http://{}:8080/is_turn?node_id={}&epoch_id={}",
        config.network.ip_manager_address, config.node.id, current_epoch
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
            error!("Error checking turn for node {}: {:?}", config.node.id, e);
            false
        }
    }
}

async fn notify_transaction_submitted(client: &Client, config: &Config, node_id: usize) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("http://{}:8080/submit_transaction", config.network.ip_manager_address);
    let payload = json!({ "node_id": node_id });
    
    let response = client.post(&url).json(&payload).send().await?;
    if response.status().is_success() {
        info!("Node {}: Successfully notified python server transaction submission.", node_id);
    } else {
        error!("Node {}: Failed to notify python server transaction submission. Status: {}", node_id, response.status());
    }
    Ok(())
}

/// Generate and send transactions in order
async fn generate_and_send_transactions_in_order(
    client: &Client,
    config: &Config,
    node: &Node,
    current_epoch: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {}: Waiting for its turn to submit transaction for epoch {}",
        node.id, current_epoch
    );

    loop {
        if is_node_turn(client, config, current_epoch).await {
            break;
        }

        // Call ensure_epoch_sync if transaction submission is stuck
        ensure_epoch_sync(client, config, current_epoch).await;

        sleep(Duration::from_secs(1)).await; // Poll every 1 second
        info!(
            "Node {}: Retrying transaction submission for epoch {}",
            config.node.id, current_epoch
        );
    }

    info!("Node {}: It's my turn. Generating transactions for epoch {}", node.id, current_epoch);

    let transaction_size = config.consensus.transaction_size;
    let data_shards = config.consensus.data_shards;

    let transaction_data = vec![1; transaction_size]; // Use deterministic data for consistency
    let shard_size = transaction_size / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let merkle_root = compute_merkle_root(&shard_hashes);

    info!(
        "Node {}: Shards: {:?}, Shard hashes: {:?}, Computed Merkle root: {:?}",
        node.id, shards, shard_hashes, merkle_root
    );

    for (index, node_url) in config.network.nodes.iter().enumerate() {
        let shard = &shards[index % shards.len()];
        let merkle_branch = compute_merkle_branch(&shard_hashes, index % shards.len());

        let payload = json!({
            "sender": node.id,
            "shard": shard,
            "proof": merkle_branch,
            "root": merkle_root,
            "epoch_id": current_epoch,
        });

        let response = client
            .post(format!("http://{}/propose", node_url))
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(res) => {
                if res.status().is_success() {
                    info!(
                        "Node {}: Successfully sent transaction for epoch {} to {}",
                        node.id, current_epoch, node_url
                    );
                } else {
                    error!(
                        "Node {}: Failed to send transaction for epoch {} to {}. Status: {}",
                        node.id, current_epoch, node_url, res.status()
                    );
                }
            }
            Err(e) => {
                error!(
                    "Node {}: Error sending transaction for epoch {} to {}: {:?}",
                    node.id, current_epoch, node_url, e
                );
            }
        }
    }

    info!(
        "Node {}: SENT Transaction proposals for epoch {}.",
        node.id, current_epoch
    );
    Ok(())
}

/// Load configuration
fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}
fn save_config(file_path: &str, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_contents = toml::to_string(&config)
        .expect("Failed to serialize configuration.");
    fs::write(file_path, config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let mut config = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();
    let current_epoch = 1;

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes));

    wait_for_all_nodes_health(&client, &config.network.nodes).await;

    if !ensure_epoch_sync(&client, &config, current_epoch).await {
        error!("Epoch synchronization failed. Exiting...");
        return Err("Epoch synchronization failed".into());
    }

    generate_and_send_transactions_in_order(&client, &config, &node, current_epoch).await?;


    // Update the proposals field
    if !config.network.proposals.contains(&node.id) {
        config.network.proposals.push(node.id);
        save_config("/home/aleph-node/aleph-node-config.toml", &config)?;
        info!("Node {}: Added to proposals in toml.", node.id);
    } else {
        info!("Node {}: Already added to proposals in toml.", node.id);
    }
 
    // Notify the Python server
    notify_transaction_submitted(&client, &config, node.id).await?;
    Ok(())
}
