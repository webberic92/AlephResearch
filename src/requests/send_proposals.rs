use base64::engine::general_purpose;
use base64::Engine;
use reqwest::{Client, StatusCode};
use sha2::Digest;
use tracing::{error, info};
use crate::{
    structs::{requests::{BaseRequest, ProposeRequest}, toml_config::TomlConfig},
    utils::merkle_utils::{compute_merkle_branch, compute_merkle_root},
};

/// Sends proposal messages to all nodes in the network.
/// According to ch-RBC, this phase involves distributing shards, Merkle proofs, and metadata.
/// Assumes all nodes are honest (no need for redundant validation).
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

    // Step 1: Compute hashes for all shards
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // Step 2: Validate the provided Merkle root
    let computed_root = compute_merkle_root(&shard_hashes);
    if computed_root != merkle_root {
        return Err(format!(
            "Computed Merkle root does not match the provided root.\nComputed: {:?}\nProvided: {:?}",
            computed_root, merkle_root
        )
        .into());
    }

    // Flag to track success for all proposals
    let mut all_successful = true;

    // Step 3: Iterate through all nodes in the network
    for node_url in &toml_config.network.nodes {
        info!(
            "Node {} {}: Sending proposal to {} for epoch {}",
            toml_config.node.id,
            toml_config.network.ip_address,
            node_url,
            toml_config.consensus.epoch_round_id
        );

        // Encode shards for transport
        let encoded_shards: Vec<String> = shards
            .iter()
            .map(|shard| general_purpose::STANDARD.encode(shard))
            .collect();

        // Compute and encode Merkle proofs for each shard
        let encoded_proofs: Vec<Vec<String>> = shard_hashes
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(&shard_hashes, i)) // Compute branch for the shard
            .map(|branch| branch.iter().map(|b| general_purpose::STANDARD.encode(b)).collect()) // Encode proof
            .collect();

        // Prepare the base request metadata
        let base_request = BaseRequest {
            sender_id: toml_config.node.id,                   // Node ID
            epoch_id: toml_config.consensus.epoch_round_id,   // Current epoch
            root: merkle_root.to_vec(),                       // Merkle root
        };

        // Construct the proposal request
        let propose_request = ProposeRequest {
            base: base_request,
            proofs: encoded_proofs,   // Merkle proofs for each shard
            shards: encoded_shards,   // Shards for the proposal
        };

        // Step 4: Send the proposal to the target node
        match client
            .post(format!("http://{}/propose", node_url)) // Target node's endpoint
            .json(&propose_request)                      // Proposal payload
            .send()
            .await
        {
            // Log success if the proposal is delivered
            Ok(res) if res.status() == StatusCode::OK => {
                info!(
                    "Node {}: Proposal successfully delivered to Node {} (Epoch {}).",
                    toml_config.node.id, node_url, toml_config.consensus.epoch_round_id
                );
            }
            // Log failure if the proposal is rejected or fails to send
            Ok(res) => {
                error!(
                    "Node {}: Proposal failed for {}. Status: {}. Response: {}",
                    toml_config.node.id,
                    node_url,
                    res.status(),
                    res.text().await.unwrap_or_else(|_| "No response body".to_string())
                );
                all_successful = false;
            }
            // Log network error
            Err(e) => {
                error!(
                    "Node {}: Network error while sending proposal to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false;
            }
        }
    }

    // Step 5: Final status check
    if all_successful {
        info!(
            "Node {} {}: Successfully sent all proposals for epoch {}.",
            toml_config.node.id,
            toml_config.network.ip_address,
            toml_config.consensus.epoch_round_id
        );
        Ok(())
    } else {
        Err("One or more proposals failed.".into())
    }
}


