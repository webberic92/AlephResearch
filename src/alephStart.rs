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

/// Compute Merkle branch for a specific index
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

/// Assign an epoch ID to a transaction
async fn assign_epoch_id(transaction: &[u8], current_epoch: &mut u64) -> Data {
    *current_epoch += 1;
    Data {
        transaction: transaction.to_vec(),
        epoch_id: *current_epoch,
    }
}

/// Ensure no overlap between epochs
async fn ensure_no_overlap(epoch_tracker: &Arc<Mutex<HashSet<u64>>>, epoch_id: u64) -> Result<(), &'static str> {
    let mut tracker = epoch_tracker.lock().await;
    if tracker.contains(&epoch_id) {
        Err("Epoch overlap detected")
    } else {
        tracker.insert(epoch_id);
        Ok(())
    }
}

/// Send proposals to nodes
async fn send_proposals(
    client: &Client,
    config: &Config,
    node: &Node,
    current_epoch: &mut u64,
    epoch_tracker: &Arc<Mutex<HashSet<u64>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender_id = config.node.id;
    let transaction_size = config.consensus.transaction_size;
    let data_shards = config.consensus.data_shards;

    // Create dummy shard data
    let shard_size = transaction_size / data_shards;
    let shards: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| vec![i as u8; shard_size])
        .collect();

    // Assign epoch ID to the transaction
    let data = assign_epoch_id(&shards.concat(), current_epoch).await;

    // Ensure no overlap
    ensure_no_overlap(epoch_tracker, data.epoch_id).await?;

    // Compute Merkle root and branches
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let merkle_root = compute_merkle_root(&shard_hashes);

    info!("Merkle root for epoch {}: {:?}", data.epoch_id, merkle_root);
    for (i, hash) in shard_hashes.iter().enumerate() {
        info!("Shard {} hash: {:?}", i, hash);
    }

    // Send proposals
    for (index, node_url) in config.network.nodes.iter().enumerate() {
        sleep(Duration::from_millis(500)).await; // Throttling requests
        let shard = &shards[index % shards.len()];
        let merkle_branch = compute_merkle_branch(&shard_hashes, index % shards.len());
        let payload = json!({
            "sender": sender_id,
            "shard": shard,
            "proof": merkle_branch,
            "root": merkle_root,
            "epoch_id": data.epoch_id,
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
                        "Node {}: Successfully sent proposal for epoch {} to {}",
                        sender_id, data.epoch_id, node_url
                    );
                } else {
                    error!(
                        "Failed to send proposal for epoch {} to {}: {}",
                        data.epoch_id, node_url, res.status()
                    );
                }
            }
            Err(e) => error!(
                "Error sending proposal for epoch {} to {}: {:?}",
                data.epoch_id, node_url, e
            ),
        }
    }

    Ok(())
}

// Helper functions
fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// Main function
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    // Load configuration
    let config_path = "/home/aleph-node/aleph-node-config.toml";
    let config = load_config(config_path);
    let client = Client::new();

    // Initialize the Node instance
    let node = Node::new(config.node.id, config.node.total_nodes);

    // Shared epoch tracking state
    let current_epoch = &mut 0u64;
    let epoch_tracker = Arc::new(Mutex::new(HashSet::new()));

    // Wait for all nodes to be healthy
    wait_for_all_nodes_health(&client, &config.network.nodes).await;

    // Send proposals with epoch tracking
    send_proposals(&client, &config, &node, current_epoch, &epoch_tracker).await?;

    Ok(())
}
