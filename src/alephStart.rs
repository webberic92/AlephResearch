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



async fn generate_and_send_transactions(
    client: &Client,
    config: &Config,
    node: &Node,
    current_epoch: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Node {}: Generating transactions for epoch {}", node.id, current_epoch);

    // Retrieve consensus parameters
    let transaction_size = config.consensus.transaction_size;
    let data_shards = config.consensus.data_shards;

    // Generate dummy transaction data (you can replace this with real data later)
    let shard_size = transaction_size / data_shards;
    let shards: Vec<Vec<u8>> = (0..data_shards)
        .map(|i| vec![i as u8; shard_size])
        .collect();

    // Compute Merkle root for the transaction
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let merkle_root = compute_merkle_root(&shard_hashes);
    info!("Node {}: Merkle root for epoch {}: {:?}", node.id, current_epoch, merkle_root);

    // Log each shard hash
    for (i, hash) in shard_hashes.iter().enumerate() {
        info!("Node {}: Shard {} hash for epoch {}: {:?}", node.id, i, current_epoch, hash);
    }

    // Broadcast transaction to all nodes
    for (index, node_url) in config.network.nodes.iter().enumerate() {
        // Ensure each node gets a unique shard
        let shard = &shards[index % shards.len()];
        let merkle_branch = compute_merkle_branch(&shard_hashes, index % shards.len());

        // Construct transaction payload
        let payload = json!({
            "sender": node.id,
            "shard": shard,
            "proof": merkle_branch,
            "root": merkle_root,
            "epoch_id": current_epoch,
        });

        // Send transaction proposal
        let response = client
            .post(format!("http://{}/propose", node_url))
            .json(&payload)
            .send()
            .await;

        // Handle response
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


    /// Compute the Merkle branch for a given index in the Merkle tree
fn compute_merkle_branch(hashes: &[Vec<u8>], index: usize) -> Vec<Vec<u8>> {
    let mut branch = vec![];
    let mut current_index = index;
    let mut current_level = hashes.to_vec();

    // Traverse up the Merkle tree to construct the branch
    while current_level.len() > 1 {
        // Determine the sibling index
        let sibling_index = if current_index % 2 == 0 {
            current_index + 1
        } else {
            current_index - 1
        };

        // Add the sibling hash to the branch if it exists
        if sibling_index < current_level.len() {
            branch.push(current_level[sibling_index].clone());
        }

        // Move up one level
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


    info!(
        "Node {}: Transactions for epoch {} broadcasted successfully",
        node.id, current_epoch
    );
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

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes));

    // Ensure all nodes are healthy
    wait_for_all_nodes_health(&client, &config.network.nodes).await;

    // Synchronize epoch states before starting proposals
    if let Err(e) = synchronize_epoch_states(&config.network.nodes, &client, current_epoch).await {
        error!("Failed to synchronize epoch {}: {}", current_epoch, e);
        return Err(e);
    }
    info!("Epoch {} synchronized successfully", current_epoch);


// Trigger transaction generation and broadcasting
    if let Err(e) = generate_and_send_transactions(&client, &config, &node, current_epoch).await {
        error!("Failed to generate or send transactions for epoch {}: {}", current_epoch, e);
        return Err(e);
    }
    info!("Transactions for epoch {} generated and broadcasted successfully", current_epoch);


        Ok(())
}
