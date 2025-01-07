use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use aleph_research::aleph_start;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::{error, info};
use tracing_subscriber;

use aleph_start::structs::{Config, Node};
use aleph_start::config_util::{load_config, save_config};
use aleph_start::merkle_util::{compute_merkle_branch, compute_merkle_root};
use aleph_start::node_health_util::wait_for_all_nodes_health;


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
