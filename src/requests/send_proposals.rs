use base64::Engine;
use base64::engine::general_purpose;
use reqwest::{Client, StatusCode};
use tracing::{error, info};

use crate::{structs::{requests::ProposeRequest, toml_config::TomlConfig}, utils::merkle_utils::compute_merkle_branch};

pub async fn send_proposals(
    client: &Client,
    toml_config: &TomlConfig,
    shards: &[Vec<u8>],
    proofs: &[Vec<u8>],
    merkle_root: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Logging shard hashes before sending proposals for epoch {}",
        toml_config.node.id,
        toml_config.network.ip_address,
        toml_config.consensus.epoch_round_id
    );

    for (i, hash) in proofs.iter().enumerate() {
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
        let merkle_branch: Vec<Vec<u8>> = compute_merkle_branch(&proofs, index % proofs.len())
            .iter()
            .map(|hash| hash.clone())
            .collect();

        let serialized_shards: Vec<String> = shards
            .iter()
            .map(|shard| general_purpose::STANDARD.encode(shard))
            .collect();

        let serialized_proofs: Vec<Vec<String>> = merkle_branch
            .iter()
            .map(|branch| {
                branch
                    .iter()
                    .map(|b| general_purpose::STANDARD.encode([*b].as_ref()))
                    .collect()
            })
            .collect();

        info!("Serialized shards for payload: {:?}", serialized_shards);
        info!("Serialized proofs for payload: {:?}", serialized_proofs);

        // Prepare ProposeRequest struct
        let propose_request = ProposeRequest {
            sender: toml_config.node.id,
            root: merkle_root.to_vec(),
            proofs: serialized_proofs,
            shards: serialized_shards,
            epoch_id: toml_config.consensus.epoch_round_id,
        };

        info!("ProposeRequest payload to node {}: {:?}", node_url, propose_request);

        // Send request
        match client.post(format!("http://{}/propose", node_url))
            .json(&propose_request)
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

