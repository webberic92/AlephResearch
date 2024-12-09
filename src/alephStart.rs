use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use tracing::{info, error};
use tracing_subscriber;

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

//TODO
// Implement the creation of shares using erasure coding.
// Compute the Merkle tree root for the shares.
// Include the actual computed Merkle branch and root in the propose message.

// Phase 1: Proposal Phase
// The sender node creates shares of the data to be broadcast using erasure coding
// and computes a Merkle tree root for the shares. Each share, along with the 
// corresponding Merkle branch, is sent to the respective recipient nodes in a 
// `propose` message. Nodes validate the size of the share to prevent malicious 
// oversized proposals.
async fn send_proposals(
    client: &Client,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender_id = config.node.id;
    let transaction_size = config.consensus.transaction_size;
    let batch_size = config.consensus.batch_size;

    for i in 0..batch_size {
        let base_transaction = format!("Transaction {} from Node {}", i + 1, sender_id);
        let transaction_data = generate_transaction(&base_transaction, transaction_size);

        for node in &config.network.nodes {
            let payload = json!({
                "sender": sender_id,
                "shard": transaction_data.as_bytes().to_vec(),
                "proof": vec![4, 5, 6], // Example proof
                "root": vec![7, 8, 9],  // Example root
            });

            // Log the payload and target URL
            info!("Sending request to: http://{}/propose", node);
            info!("Payload: {:?}", payload);

            let response = client
                .post(format!("http://{}/propose", node))
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(res) => {
                    if res.status().is_success() {
                        info!("Response from {}: {:?}", node, res.text().await?);
                    } else {
                        error!(
                            "Request failed to {} with status: {}",
                            node,
                            res.status()
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "Request to {} failed with error: {:?}",
                        node, e
                    );
                }
            }
        }
    }

    Ok(())
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

    // Send proposals using the extracted function
    send_proposals(&client, &config).await?;

    Ok(())
}
