use base64::Engine;
use base64::engine::general_purpose;
use reqwest::{Client, StatusCode};
use sha2::Digest;
use tracing::{error, info};
use crate::{
    structs::{requests::ProposeRequest, toml_config::TomlConfig},
    utils::merkle_utils::{compute_merkle_branch, compute_merkle_root},
};

pub async fn send_proposals(
    client: &Client,
    toml_config: &TomlConfig,
    shards: &[Vec<u8>],
    merkle_root: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Preparing to send proposals for epoch {}",
        toml_config.node.id,
        toml_config.network.ip_address,
        toml_config.consensus.epoch_round_id
    );

    // Compute shard hashes
    let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|shard| sha2::Sha256::digest(shard).to_vec()).collect();

    // Validate Merkle root
    let computed_root = compute_merkle_root(&shard_hashes);
    if computed_root != merkle_root {
        return Err(format!(
            "Computed Merkle root does not match the provided root. Computed: {:?}, Provided: {:?}",
            computed_root, merkle_root
        )
        .into());
    }

    let mut all_successful = true;

    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {
        info!(
            "Node {} {}: Sending proposal to {} for epoch {}",
            toml_config.node.id,
            toml_config.network.ip_address,
            node_url,
            toml_config.consensus.epoch_round_id
        );

        // Compute Merkle branch for this node
        // let merkle_branch = compute_merkle_branch(&shard_hashes, index);

        // Encode shards and Merkle branch to Base64 strings
        let encoded_shards: Vec<String> = shards.iter().map(|shard| general_purpose::STANDARD.encode(shard)).collect();
        let encoded_proofs: Vec<Vec<String>> = shard_hashes
        .iter()
        .enumerate()
        .map(|(i, _)| compute_merkle_branch(&shard_hashes, i))
        .map(|branch| branch.iter().map(|b| general_purpose::STANDARD.encode(b)).collect())
        .collect();
        
        // Prepare the ProposeRequest
        let propose_request = ProposeRequest {
            senderId: toml_config.node.id,
            root: merkle_root.to_vec(),
            proofs: encoded_proofs, // No additional wrapping
            shards: encoded_shards,
            epoch_id: toml_config.consensus.epoch_round_id,
        };

        // Send the request
        match client
            .post(format!("http://{}/propose", node_url))
            .json(&propose_request)
            .send()
            .await
        {
            Ok(res) if res.status() == StatusCode::OK => {
                info!(
                    "Node {}: Proposal successfully delivered to Node {} (Epoch {})",
                    toml_config.node.id, node_url, toml_config.consensus.epoch_round_id
                );
            }
            Ok(res) => {
                error!(
                    "Node {}: Proposal failed for {}: {}",
                    toml_config.node.id,
                    node_url,
                    res.text().await.unwrap_or_else(|_| "No response body".to_string())
                );
                all_successful = false;
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
            "Node {} {}: Successfully sent all proposals for epoch {}.",
            toml_config.node.id,
            toml_config.network.ip_address,
            toml_config.consensus.epoch_round_id
        );
        Ok(())
    } else {
        Err("One or more proposals failed".into())
    }
}
