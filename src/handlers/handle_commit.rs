use std::sync::Arc;

use base64::Engine;
use reqwest::Client;
use sha2::Digest;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{error, info};
use crate::structs::node::Node;
use crate::structs::requests:: CommitRequest;
use crate::utils::dag_utils::are_parents_available;
use crate::utils::epoch_utils::{broadcast_epoch_update, update_local_epoch};
// use crate::utils::epoch_utils::update_epoch_to_next_round;
use crate::utils::merkle_utils::validate_merkle_branch;

/// Handles the commit phase in the Aleph protocol based on the ch-RBC proof.
pub async fn handle_commit(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    // Log the incoming request
    let node_id = node.read().await.id;
    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node_id, commit_request.base.sender_id, commit_request.base.epoch_id
    );

    // Step 1: Derive shard hashes from the unit
    let shard_size = 64; // Size of each shard
    let shard_hashes: Vec<Vec<u8>> = commit_request
        .unit
        .chunks(shard_size)
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
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
                    node_id, index, e
                );
                error!("{}", error_message);
                error_message
            })?;

        if !validate_merkle_branch(&shard_hashes, &decoded_proof, index, &commit_request.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}",
                node_id, index, commit_request.base.root
            );
            error!("{}", error_message);
            return Err(error_message);
        }
    }
    info!("Node {}: Merkle branch validation passed for root {:?}", node_id, commit_request.base.root);

    // Step 3: Check parent availability
    if !are_parents_available(node.clone(), &commit_request.unit).await {
        let error_message = format!(
            "Node {}: Parent availability check failed for unit associated with root {:?}",
            node_id, commit_request.base.root
        );
        error!("{}", error_message);
        return Err(error_message);
    }
    info!("Node {}: Parent availability check passed for root {:?}", node_id, commit_request.base.root);

    // Step 4: Persist the finalized unit to storage
    let epoch_dir = "/home/aleph-node/logs/finalized_units";
    let epoch_file = format!("{}/epoch{}.json", epoch_dir, commit_request.base.epoch_id);

    info!("Node {}: Persisting finalized unit to file: {}", node_id, epoch_file);

    if let Err(e) = tokio::fs::create_dir_all(epoch_dir).await {
        let error_message = format!(
            "Node {}: Failed to create directory for finalized units: {:?}",
            node_id, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    let unit_entry = serde_json::json!({
        "node_id": node_id,
        "sender": commit_request.base.sender_id,
        "root": commit_request.base.root.clone(),
        "unit": commit_request.unit,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if let Err(e) = write_finalized_unit(&epoch_file, unit_entry).await {
        let error_message = format!(
            "Node {}: Failed to write finalized unit to file {}: {:?}",
            node_id, epoch_file, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    // Step 5: Update the epoch to the next round
    let new_epoch_id = update_local_epoch(node.clone()).await;
    // Broadcast the epoch update to other nodes
    if let Err(e) = broadcast_epoch_update(node.clone(), client.clone(), new_epoch_id).await {
        error!("Failed to broadcast epoch update: {}", e);
    }

    info!("Node {}: Successfully handled commit request.", node_id);
    Ok(())
}

// Writes the finalized unit to the epoch file.
async fn write_finalized_unit(
    epoch_file: &str,
    unit_entry: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut epoch_data = match fs::read_to_string(epoch_file).await {
        Ok(content) => serde_json::from_str::<Vec<serde_json::Value>>(&content).unwrap_or_else(|_| vec![]),
        Err(_) => vec![],
    };

    epoch_data.push(unit_entry);

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(epoch_file)
        .await?;
    file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes())
        .await?;

    Ok(())
}