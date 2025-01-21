use reqwest::Client;
use tracing::{error, info};

use crate::{structs::{requests::PrevoteRequest, toml_config::TomlConfig}, utils::merkle_utils::compute_merkle_branch};
pub async fn send_prevotes(
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