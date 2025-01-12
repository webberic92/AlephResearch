use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::{error, info};
use tracing_subscriber;

use aleph_research::structs;
use crate::structs::toml_config::TomlConfig;
use crate::structs::node::Node;

use aleph_research::utils::config_util::{load_config, save_config};
use aleph_research::utils::ip_server_utils::{is_node_turn, notify_transaction_submitted};
use aleph_research::utils::rbc_utils::{wait_for_all_nodes_health, ensure_epoch_sync};
use aleph_research::utils::merkle_utils::{compute_merkle_branch, compute_merkle_root};

/// Main function to generate and send transactions in order
async fn generate_and_send_transactions_in_order(
    client: &Client,
    toml_config: &TomlConfig,
    node: &Node,
    current_epoch: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    wait_for_turn(client, toml_config, node, current_epoch).await?;

    let (shards, shard_hashes, merkle_root) = generate_shards_and_merkle_root(toml_config).await;

    send_transactions(client, toml_config, node, current_epoch, &shards, &shard_hashes, &merkle_root).await?;

    info!(
        "Node {}: SENT Transaction proposals for epoch {}.",
        node.id, current_epoch
    );
    Ok(())
}

/// Wait for the node's turn to submit a transaction
async fn wait_for_turn(
    client: &Client,
    toml_config: &TomlConfig,
    node: &Node,
    current_epoch: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {}: Waiting for its turn to submit transaction for epoch {}",
        node.id, current_epoch
    );

    loop {
        if is_node_turn(client, toml_config, current_epoch).await {
            break;
        }

        ensure_epoch_sync(client, toml_config, current_epoch).await;
        sleep(Duration::from_secs(1)).await; // Poll every 1 second
        info!(
            "Node {}: Retrying transaction submission for epoch {}",
            toml_config.node.id, current_epoch
        );
    }

    info!("Node {}: It's my turn for epoch {}", node.id, current_epoch);
    Ok(())
}

/// Generate shards and Merkle root
async fn generate_shards_and_merkle_root(
    toml_config: &TomlConfig,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>) {
    let transaction_size = toml_config.consensus.transaction_size;
    let data_shards = toml_config.consensus.data_shards;

    let transaction_data = vec![1; transaction_size]; // Deterministic data
    let shard_size = transaction_size / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let merkle_root = compute_merkle_root(&shard_hashes);

    info!(
        "Generated shards: {:?}, shard hashes: {:?}, Merkle root: {:?}",
        shards, shard_hashes, merkle_root
    );

    (shards, shard_hashes, merkle_root)
}

/// Send transactions to other nodes
async fn send_transactions(
    client: &Client,
    toml_config: &TomlConfig,
    node: &Node,
    current_epoch: u64,
    shards: &[Vec<u8>],
    shard_hashes: &[Vec<u8>],
    merkle_root: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {
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
                        "Node {}:***========== SUCCESSFULLY SENT PROPOSE REQUEST for epoch {} to {}=======***",
                        node.id, current_epoch, node_url
                    );
                } else {
                    error!(
                        "Node {}: Failed to send propose request transaction for epoch {} to {}. Status: {}",
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

    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let mut toml_config = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();
    let current_epoch = 1;

    let node = Arc::new(Node::new(toml_config.node.id, toml_config.node.total_nodes));

    wait_for_all_nodes_health(&client, &toml_config.network.nodes).await;

    if !ensure_epoch_sync(&client, &toml_config, current_epoch).await {
        error!("Epoch synchronization failed. Exiting...");
        return Err("Epoch synchronization failed".into());
    }

    generate_and_send_transactions_in_order(&client, &toml_config, &node, current_epoch).await?;

    // Update the proposals field
    if !toml_config.network.proposals.contains(&node.id) {
        toml_config.network.proposals.push(node.id);
        save_config("/home/aleph-node/aleph-node-config.toml", &toml_config)?;
        info!("Node {}: Added to proposals in toml. Current proposals = {:?}", node.id, toml_config.network.proposals);
    } else {
        info!("Node {}: Already added to proposals in toml. Current proposals = {:?}", node.id, toml_config.network.proposals);
    }

    // Notify the Python server
    notify_transaction_submitted(&client, &toml_config, node.id).await?;
    Ok(())
}
