use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use tracing::{info, error};
use tracing_subscriber;
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};
use std::time::Duration;

// Configuration structure to parse the TOML file
#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    consensus: ConsensusConfig,
    logging: LoggingConfig,
    node: NodeConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    listen_address: String,
    nodes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConsensusConfig {
    batch_size: usize,
    transaction_size: usize,
    round: usize,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    transaction_metrics_log: String,
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    id: usize,
    total_nodes: usize,
}

// Function to load the configuration from the TOML file
fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// Helper function to generate a transaction of exactly the specified size
fn generate_transaction(data: &str, size: usize) -> String {
    if data.len() >= size {
        data[..size].to_string() // Truncate if too long
    } else {
        let padding = "x".repeat(size - data.len());
        format!("{}{}", data, padding) // Pad if too short
    }
}


// Implement the creation of shares using erasure coding.
// Compute the Merkle tree root for the shares.
// Include the actual computed Merkle branch and root in the propose message.

// Phase 1: Proposal Phase
// The sender node creates shares of the data to be broadcast using erasure coding
// and computes a Merkle tree root for the shares. Each share, along with the 
// corresponding Merkle branch, is sent to the respective recipient nodes in a 
// `propose` message. Nodes validate the size of the share to prevent malicious 
// oversized proposals.
// Helper function to compute the Merkle root
async fn send_proposals(
    client: &Client,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender_id = config.node.id;
    let transaction_size = config.consensus.transaction_size;
    let batch_size = config.consensus.batch_size;

    // Erasure coding parameters
    let data_shards = 4;
    let parity_shards = 2;
    let total_shards = data_shards + parity_shards;
    let rs = ReedSolomon::new(data_shards, parity_shards).unwrap();

    // Generate the batch of transactions as a single payload
    let mut batch_data = String::new();
    for i in 0..batch_size {
        let transaction = format!("Transaction {} : from Node {}\n", i + 1, sender_id);
        batch_data.push_str(&generate_transaction(&transaction, transaction_size));
    }

    // Prepare the shards
    let shard_size = (batch_data.len() + data_shards - 1) / data_shards;
    let mut shards: Vec<Vec<u8>> = vec![vec![0; shard_size]; total_shards];

    // Fill data shards
    for (i, chunk) in batch_data.as_bytes().chunks(shard_size).enumerate() {
        shards[i][..chunk.len()].copy_from_slice(chunk);
    }

    // Encode parity shards
    rs.encode(&mut shards).unwrap();

    // Compute Merkle tree
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec()) // Hash each shard
        .collect();
    let merkle_root = compute_merkle_root(&shard_hashes);

    // Send exactly one propose message to each node
    for (index, node) in config.network.nodes.iter().enumerate() {
        let shard = &shards[index % shards.len()];
        let merkle_branch = compute_merkle_branch(&shard_hashes, index % shards.len());

        let payload = json!({
            "sender": sender_id,
            "shard": shard,
            "proof": merkle_branch,
            "root": merkle_root,
        });

        // Log the payload and target URL
        info!("Node {} : Sending request to: http://{}/propose", sender_id, node);
        info!("Payload: {:?}", payload);

        let response = client
            .post(format!("http://{}/propose", node))
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(res) => {
                if res.status().is_success() {
                    info!("Node {} : Response from {}: {:?}", node, sender_id, res.text().await?);
                } else {
                    error!(
                        "Node {} : Request failed to {} with status: {}",
                        sender_id,
                        node,
                        res.status()
                    );
                }
            }
            Err(e) => {
                error!(
                    "Node {} : Request to {} failed with error: {:?}",
                    sender_id, node, e
                );
            }
        }
    }

    Ok(())
}

// Helper function to compute Merkle root
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

// Helper function to compute Merkle branch
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
        info!("Some nodes are not healthy. Retrying in 5 seconds...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration from TOML
    let config = load_config("/home/aleph-node/aleph-node-config.toml");

    // HTTP client
    let client = Client::new();

    // Wait for all nodes to be healthy
    wait_for_all_nodes_health(&client, &config.network.nodes).await;

    // Send proposals using the extracted function
    send_proposals(&client, &config).await?;

    Ok(())
}
