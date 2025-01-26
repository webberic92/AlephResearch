use std::path::Path;
use std::sync::Arc;
use base64::Engine;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use serde_json::json;
use tracing::{error, info};
use crate::requests::ip_server_requests::notify_transaction_submitted;
use crate::structs::node::Node;
use crate::structs::requests::{BaseRequest, CommitRequest};
use crate::structs::toml_config;
use crate::utils::config_util::load_config;
use crate::utils::dag_utils::{are_parents_available, validate_unit};
use crate::utils::epoch_utils::update_epoch_to_next_round;
use crate::utils::merkle_utils::validate_merkle_branch;

/// Handles the commit phase in the Aleph protocol based on the ch-RBC proof.

pub async fn handle_commit(
    node: &Node,
    client: Arc<Client>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node.id, commit_request.base.sender_id, commit_request.base.epoch_id
    );

    // Step 1: Derive shard hashes from the unit
    let shard_size = 64; // Adjust this based on configuration
    let shard_hashes: Vec<Vec<u8>> = commit_request
        .unit
        .chunks(shard_size)
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    // Step 2: Validate Merkle branches for each shard
    for (index, proof) in commit_request.proofs.iter().enumerate() {
        let decoded_proof: Vec<Vec<u8>> = proof
            .iter()
            .map(|p| base64::engine::general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|e| {
                let error_message = format!(
                    "Node {}: Failed to decode proof for shard {}: {:?}",
                    node.id, index, e
                );
                error!("{}", error_message);
                error_message
            })?;

        if !validate_merkle_branch(&shard_hashes, &decoded_proof, index, &commit_request.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}",
                node.id, index, commit_request.base.root
            );
            error!("{}", error_message);
            return Err(error_message);
        }
    }

    info!("Node {}: Unit validation passed for root {:?}", node.id, commit_request.base.root);

    // Step 3: Check if parents of the unit are available
    info!("Node {}: Checking parent availability for the unit.", node.id);
    if !are_parents_available(node, &commit_request.unit).await {
        let error_message = format!(
            "Node {}: Parent availability check failed for unit associated with root {:?}",
            node.id, commit_request.base.root
        );
        error!("{}", error_message);
        return Err(error_message);
    }
    info!("Node {}: Parent availability check passed.", node.id);

    // Step 4: Persist the finalized unit to storage
    let epoch_dir = "./finalized_units";
    let epoch_file = format!("{}/epoch{}.json", epoch_dir, commit_request.base.epoch_id);
    info!("Node {}: Persisting finalized unit to file: {}", node.id, epoch_file);

    if let Err(e) = fs::create_dir_all(Path::new(epoch_dir)).await {
        let error_message = format!(
            "Node {}: Failed to create directory for finalized units: {:?}",
            node.id, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    if let Err(e) = append_finalized_unit(
        &epoch_file,
        node.id,
        commit_request.base.sender_id,
        commit_request.base.root.clone(),
        commit_request.unit,
    )
    .await
    {
        let error_message = format!(
            "Node {}: Failed to append finalized unit to file {}: {:?}",
            node.id, epoch_file, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {}: Successfully finalized unit for root {:?} and persisted it to file.",
        node.id, commit_request.base.root
    );
    let toml_config = load_config();
    update_epoch_to_next_round(&client).await; 
    let _ = notify_transaction_submitted(&client, &toml_config).await;

    Ok(())
}

/// Appends a finalized unit to the epoch file.
async fn append_finalized_unit(
    epoch_file: &str,
    node_id: usize,
    sender: usize,
    root: Vec<u8>,
    unit: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Node {}: Reading existing data from file: {}", node_id, epoch_file);
    let mut epoch_data = match fs::read_to_string(epoch_file).await {
        Ok(content) => {
            info!("Node {}: Successfully read existing data from file.", node_id);
            serde_json::from_str::<Vec<serde_json::Value>>(&content).unwrap_or_else(|_| vec![])
        }
        Err(_) => {
            info!("Node {}: No existing data found. Initializing new epoch data.", node_id);
            vec![]
        }
    };

    let unit_entry = json!({
        "node_id": node_id,
        "sender": sender,
        "root": root,
        "unit": unit,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    epoch_data.push(unit_entry);

    info!("Node {}: Writing updated data to file: {}", node_id, epoch_file);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(epoch_file)
        .await?;
    file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes())
        .await?;
    info!("Node {}: Successfully wrote data to file.", node_id);

    Ok(())
}

