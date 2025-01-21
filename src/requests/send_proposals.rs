use base64::Engine;
use base64::engine::general_purpose;
use reqwest::{Client, StatusCode};
use serde_json::json;
use tracing::{error, info};

use crate::{structs::toml_config::TomlConfig, utils::merkle_utils::compute_merkle_branch};

pub async fn send_proposals(
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
        info!(
            "Node {} {}: SENT Transaction proposals for epoch {}.",
            toml_config.node.id, toml_config.network.ip_address, toml_config.consensus.epoch_round_id
        );
        Ok(())
    } else {
        Err("One or more proposals failed".into())
    }
}