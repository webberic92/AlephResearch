
use aleph_research::structs::requests::PrevoteRequest;
use aleph_research::structs::toml_config::TomlConfig;
use reqwest::{Client, StatusCode};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use tracing_subscriber;
use aleph_research::utils::config_util::{are_enough_proposals_received, load_config, save_config};
use aleph_research::utils::ip_server_utils::{is_node_turn, notify_transaction_submitted};
use aleph_research::utils::rbc_utils::{wait_for_all_nodes_health, ensure_epoch_sync};
use aleph_research::utils::merkle_utils::{compute_merkle_branch, compute_merkle_root};
use base64::{engine::general_purpose, Engine as _};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let toml_config: TomlConfig = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();

    wait_for_all_nodes_health(&client, &toml_config).await;

    // Handle the Result from generate_and_send_transactions_in_order
    let (merkle_root, proofs, shards) = generate_and_send_transactions_in_order(&client, &toml_config).await?;

    // Update the proposals field in the TOML config
    let mut updated_toml_config = load_config("/home/aleph-node/aleph-node-config.toml");
    if !updated_toml_config.network.proposals.contains(&updated_toml_config.node.id) {
        updated_toml_config.network.proposals.push(updated_toml_config.node.id);
        save_config("/home/aleph-node/aleph-node-config.toml", &updated_toml_config)?;
        info!(
            "Node {} {}: Added to proposals. Current proposals: {:?}",
            updated_toml_config.node.id, updated_toml_config.network.ip_address, updated_toml_config.network.proposals
        );
    }

    // Pass the unwrapped merkle_root to send_prevotes
    if are_enough_proposals_received().await {
        info!(
            "Node {} {}: Enough proposals. received to start prevoting form aleph_start.{:?}",
            updated_toml_config.node.id, updated_toml_config.network.ip_address, updated_toml_config.network.proposals
        );
        // send_prevotes(&client, &updated_toml_config, &merkle_root, &proofs, &shards).await?;
    }

    // notify_transaction_submitted(&client, &updated_toml_config).await?;
    Ok(())
}



/// Prevote logic
async fn send_prevotes(
    client: &Client,
    toml_config: &TomlConfig,
    merkle_root: &Vec<u8>,
    proofs: &Vec<Vec<u8>>, // Updated to plural
    shards: &Vec<Vec<u8>>, // Updated to plural
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Starting PREVOTE phase from aleph_start...",
        toml_config.node.id, toml_config.network.ip_address
    );

    let mut all_successful = true; // Track if all prevote messages succeed

    // Multicast prevote message to all nodes
    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {
        let shard = &shards[index % shards.len()];
        let merkle_branch: Vec<Vec<u8>> = compute_merkle_branch(&proofs, index % proofs.len())
            .iter()
            .map(|hash| hash.to_vec())
            .collect();

        // Construct prevote payload
        let payload = PrevoteRequest {
            sender: toml_config.node.id,
            epoch_id: toml_config.consensus.epoch_round_id,
            root: merkle_root.clone(),         // Raw byte array
            proofs: vec![merkle_branch],       // Raw byte arrays as Vec<Vec<Vec<u8>>>
            shards: vec![shard.clone()],       // Raw byte arrays
            node_url: node_url.to_string(),    // Include the node URL
        };

        info!(
            "Node {}: Sending prevote to {}. Payload: {:?}",
            toml_config.node.id, node_url, payload
        );

        let url = format!("http://{}/prevote", node_url);
        let response = client.post(&url).json(&payload).send().await;

        match response {
            Ok(res) => {
                let status = res.status();
                let response_body = res.text().await.unwrap_or_else(|_| "Failed to read response body".to_string());

                if status.is_success() {
                    info!(
                        "Node {}: Prevote successfully sent to {}. Response: {}",
                        toml_config.node.id, node_url, response_body
                    );
                } else {
                    error!(
                        "Node {}: Failed to send prevote to {}. Status: {}. Response: {}",
                        toml_config.node.id, node_url, status, response_body
                    );
                    all_successful = false; // Mark failure if any message fails
                }
            }
            Err(e) => {
                error!(
                    "Node {}: Error sending prevote to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false; // Mark failure if there's a network error
            }
        }
    }

    // Final log and return result based on success/failure
    if all_successful {
        info!(
            "Node {} {}: All PREVOTE messages sent successfully.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Ok(())
    } else {
        error!(
            "Node {} {}: Failed to send one or more PREVOTE messages.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Err("One or more PREVOTE messages failed".into())
    }
}



/// Generate and send transactions in order
async fn generate_and_send_transactions_in_order(
    client: &Client,
    toml_config: &TomlConfig,
) -> Result<(Vec<u8>, Vec<Vec<u8>>, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
    wait_for_turn(client, toml_config).await?;

    let (shards, proofs, merkle_root) = generate_shards_and_merkle_root(toml_config).await;

    send_transactions(client, toml_config, &shards, &proofs, &merkle_root).await?;

    info!(
        "Node {} {}: SENT Transaction proposals for epoch {}.",
        toml_config.node.id, toml_config.network.ip_address, toml_config.consensus.epoch_round_id
    );

    // Return Merkle root, proofs, and shards
    Ok((merkle_root, proofs, shards))
}

/// Wait for the node's turn to submit a transaction
async fn wait_for_turn(
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

        ensure_epoch_sync(client, toml_config).await;
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

async fn send_transactions(
    client: &Client,
    toml_config: &TomlConfig,
    shards: &[Vec<u8>],
    shard_hashes: &[Vec<u8>],
    merkle_root: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Logging shard hashes before sending proposals for epoch {}",
        toml_config.node.id,
        toml_config.network.ip_address,
        toml_config.consensus.epoch_round_id
    );

    for (i, hash) in shard_hashes.iter().enumerate() {
        info!(
            "Node {} {}: Shard {} Hash: {:?}",
            toml_config.node.id, toml_config.network.ip_address, i, hash
        );
    }

    let mut all_successful = true;

    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {
        info!(
            "Node {} {}: Sending proposal to {} for epoch {}",
            toml_config.node.id, toml_config.network.ip_address, node_url, toml_config.consensus.epoch_round_id
        );

        // Extract shard and corresponding Merkle branch
        let shard = &shards[index % shards.len()];
        let merkle_branch: Vec<Vec<u8>> = compute_merkle_branch(&shard_hashes, index % shard_hashes.len())
            .iter()
            .map(|hash| hash.clone())
            .collect();

        let serialized_shards: Vec<String> = shards
            .iter()
            .map(|shard| general_purpose::STANDARD.encode(shard))
            .collect();

        let serialized_proofs: Vec<Vec<String>> = shard_hashes
            .iter()
            .map(|hash| vec![general_purpose::STANDARD.encode(hash)])
            .collect();

        info!("Serialized shards for transmission: {:?}", serialized_shards);
        info!("Serialized proofs for transmission: {:?}", serialized_proofs);

        // Prepare payload
        let payload = json!({
            "sender": toml_config.node.id,
            "shards": vec![shard.clone()],
            "proofs": vec![merkle_branch],
            "root": merkle_root.to_vec(),
            "epoch_id": toml_config.consensus.epoch_round_id,
        });

        info!("Payload to node {}: {:?}", node_url, payload);

        // Send request
        match client.post(format!("http://{}/propose", node_url))
            .json(&payload)
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                let response_body = res.text().await.unwrap_or_else(|_| "Failed to read response body".to_string());

                match status {
                    StatusCode::OK => {
                        info!(
                            "Node {}: Proposal successfully sent to {}",
                            toml_config.node.id, node_url
                        );
                    }
                    StatusCode::BAD_REQUEST => {
                        error!(
                            "Node {}: Proposal rejected by {}: {}",
                            toml_config.node.id, node_url, response_body
                        );
                        all_successful = false;
                    }
                    _ => {
                        error!(
                            "Node {}: Unexpected error from {}: {}",
                            toml_config.node.id, node_url, response_body
                        );
                        all_successful = false;
                    }
                }
            }
            Err(e) => {
                error!(
                    "Node {}: Network error while sending proposal to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false;
            }
        }
    }

    if all_successful {
        Ok(())
    } else {
        Err("One or more proposals failed".into())
    }
}



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

    assert_eq!(
        shards.len(),
        data_shards,
        "Shard count mismatch: expected {}, found {}",
        data_shards,
        shards.len()
    );

    info!("Generated transaction data: {:?}", transaction_data);
    info!("Generated shards: {:?}", shards);

    let proofs: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
    let merkle_root = compute_merkle_root(&proofs);

    info!("Generated proofs: {:?}", proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    (shards, proofs, merkle_root)
}