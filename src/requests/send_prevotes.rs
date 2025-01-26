use reqwest::Client;
use tracing::{error, info};
use crate::{
    structs::{requests::{BaseRequest, PrevoteRequest, ProposeRequest}, toml_config::TomlConfig}, utils::merkle_utils::compute_merkle_branch,
};
use base64::{engine::general_purpose, Engine};

pub async fn send_prevotes(
    client: &Client,
    toml_config: &TomlConfig,
    merkle_root: &Vec<u8>,
    shards: &Vec<Vec<u8>>, // Data shards
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Starting SEND PREVOTE phase",
        toml_config.node.id, toml_config.network.ip_address
    );

    let mut all_successful = true;

    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {

        // Extract the shard for this node
        let shard = shards.get(index % shards.len()).cloned().unwrap_or_else(|| vec![]);

        // Base64-encode proofs and shard
        let encoded_proofs: Vec<Vec<String>> = shard
        .iter()
        .enumerate()
        .map(|(i, _)| compute_merkle_branch(&shards, i))
        .map(|branch| branch.iter().map(|b| general_purpose::STANDARD.encode(b)).collect())
        .collect();
        let encoded_shard = general_purpose::STANDARD.encode(&shard);


        let payload = PrevoteRequest {
            propose: ProposeRequest {
                base: BaseRequest {
                    sender_id: toml_config.node.id.clone(),
                    epoch_id: toml_config.consensus.epoch_round_id,
                    root: merkle_root.clone(),
                },
                proofs: encoded_proofs,
                shards: vec![encoded_shard],
            },
            sender_url: toml_config.network.ip_address.clone(),
        };

        info!(
            "Node {} {}: Sending prevote to {}.",
            toml_config.node.id, toml_config.network.ip_address, node_url, 
        );

        // Send the request
        let response = client
            .post(format!("http://{}/prevote", node_url))
            .json(&payload)
            .send()
            .await;

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
                    "Node {}: Error sending prevote to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false;
            }
        }
    }

    // Check if all messages were sent successfully
    if all_successful {
        info!(
            "Node {} {}: All PREVOTE messages sent successfully.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Ok(())
    } else {
        Err("One or more PREVOTE messages failed".into())
    }
}
