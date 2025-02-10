use reqwest::Client;
use tracing::{error, info};
use crate::{
    structs::{
        requests::{BaseRequest, PrevoteRequest, ProposeRequest},
        toml_config::TomlConfig,
    },
    utils::merkle_utils::compute_merkle_branch,
};
use base64::{engine::general_purpose, Engine};

/// Sends prevote messages to all nodes in the network.
///
/// # Parameters
/// - `client`: HTTP client for sending requests.
/// - `toml_config`: Configuration containing node and network details.
/// - `merkle_root`: Merkle root of the shards.
/// - `shards`: Data shards to be sent.
///
/// # Returns
/// - `Ok(())` if all prevote messages are sent successfully.
/// - `Err` if one or more messages fail.
pub async fn send_prevotes(
    client: &Client,
    toml_config: &TomlConfig,
    merkle_root: &Vec<u8>,
    shards: &Vec<Vec<u8>>, // Data shards
    parents: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Starting SEND PREVOTE phase",
        toml_config.node.id, toml_config.network.ip_address
    );

    // Flag to track the success of all messages
    let mut all_successful = true;

    // Iterate through all nodes in the network
    for node_url in &toml_config.network.nodes {
        info!(
            "Node {} {}: Preparing to send prevote to {}.",
            toml_config.node.id, toml_config.network.ip_address, node_url
        );

        // Select a shard for this node (round-robin logic)
        let shard = shards.get(0).cloned().unwrap_or_else(Vec::new); // Simplified for honest nodes

        // Step 1: Compute and encode Merkle proofs
        let encoded_proofs: Vec<Vec<String>> = shards
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(&shards, i)) // Compute Merkle branch for each shard
            .map(|branch| branch.iter().map(|b| general_purpose::STANDARD.encode(b)).collect()) // Encode proofs
            .collect();

        // Step 2: Encode shard for transport
        let encoded_shard = general_purpose::STANDARD.encode(&shard);

        // Step 3: Construct the prevote payload
        let payload = PrevoteRequest {
            propose: ProposeRequest {
                base: BaseRequest {
                    proposing_node_id: toml_config.node.id,                   // Sender's node ID
                    epoch_id: toml_config.consensus.epoch_round_id,   // Current epoch
                    root: merkle_root.clone(),                        // Merkle root
                },
                proofs: encoded_proofs,    // Merkle proofs for shards
                shards: vec![encoded_shard], // Encoded shards
                parents: parents.clone(), // Parent units
            },
            sender_url: toml_config.network.ip_address.clone(), // Sender's IP address
        };

        // Step 4: Send the prevote request to the node
        let response = client
            .post(format!("http://{}/prevote", node_url)) // Target node's prevote endpoint
            .json(&payload)                               // Payload for the request
            .send()
            .await;

        // Step 5: Handle the response
        match response {
            Ok(res) => {
                if res.status().is_success() {
                    info!(
                        "Node {}: Prevote successfully sent to {}.",
                        toml_config.node.id, node_url
                    );
                } else {
                    error!(
                        "Node {}: Prevote failed for {}. Status: {}. Response: {}",
                        toml_config.node.id,
                        node_url,
                        res.status(),
                        res.text().await.unwrap_or_else(|_| "No response body".to_string())
                    );
                    all_successful = false;
                }
            }
            Err(e) => {
                error!(
                    "Node {}: Network error while sending prevote to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false;
            }
        }
    }

    // Final check: Return success or failure
    if all_successful {
        info!(
            "Node {} {}: All PREVOTE messages sent successfully.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Ok(())
    } else {
        Err("One or more PREVOTE messages failed.".into())
    }
}



