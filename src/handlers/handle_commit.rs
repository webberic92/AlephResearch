use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use reqwest::Client;
use serde_json::Value;
use sha2::Digest;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{error, info};
use crate::structs::node::Node;
use crate::structs::requests:: CommitRequest;
use crate::utils::dag_utils::are_parents_available;
use crate::utils::epoch_utils::{broadcast_epoch_update, update_local_epoch};
// use crate::utils::epoch_utils::update_epoch_to_next_round;
use crate::utils::merkle_utils::validate_merkle_branch;

/// Handles the commit phase in the Aleph protocol based on the ch-RBC proof.
/// Handles the commit phase in the Aleph protocol based on the ch-RBC proof.
pub async fn handle_commit(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    // Step 1: Read Node ID (Drop lock after reading)
    let node_id;
    {
        let node_state = match timeout(Duration::from_secs(5), node.read()).await {
            Ok(state) => state,
            Err(_) => {
                error!("Timeout while acquiring read lock!");
                return Err("Timeout while acquiring read lock".to_string());
            }
        };
        node_id = node_state.id;
    } // 🔥 Dropping read lock

    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node_id, commit_request.base.proposing_node_id, commit_request.base.epoch_id
    );

    // Step 2: Validate Merkle branches (NO LOCK HELD)
    let shard_size = 64;
    let shard_hashes: Vec<Vec<u8>> = commit_request
        .unit
        .chunks(shard_size)
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    for (index, proof) in commit_request.proofs.iter().enumerate() {
        let decoded_proof: Vec<Vec<u8>> = proof
            .iter()
            .map(|p| base64::engine::general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|e| {
                let error_message = format!("Node {}: Failed to decode proof {}: {:?}", node_id, index, e);
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

    // Step 3: Check parent availability (NO LOCK HELD)
    if !are_parents_available(node.clone(), &commit_request.unit).await {
        let error_message = format!(
            "Node {}: Parent availability check failed for root {:?}",
            node_id, commit_request.base.root
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    // Step 4: Persist finalized unit (NO LOCK HELD)
    let epoch_file = format!("/home/aleph-node/logs/finalized_units/epoch{}.json", commit_request.base.epoch_id);
    let unit_entry = serde_json::json!({
        "committing_node_id": node_id,
        "proposer_node_id": commit_request.base.proposing_node_id,
        "merkle_root": commit_request.base.root.clone(),
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

    // Step 5: Update epoch (NO LOCK HELD)
    let new_epoch_id = update_local_epoch(node.clone()).await;

    // Step 6: Broadcast epoch update (NO LOCK HELD)
    if let Err(e) = broadcast_epoch_update(node.clone(), client.clone(), new_epoch_id).await {
        error!("Failed to broadcast epoch update: {}", e);
    }

    info!("Node {}: Successfully handled commit request.", node_id);
    Ok(())
}

// Writes the finalized unit to the epoch file.
async fn write_finalized_unit(
    epoch_file: &str,
    unit_entry: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(epoch_file);
    
    // Ensure parent directory exists
    if let Some(parent_dir) = path.parent() {
        if !parent_dir.exists() {
            println!("Creating directory: {:?}", parent_dir);
            fs::create_dir_all(parent_dir).await?;
        }
    }

    // Read existing file content or create a new vector
    let mut epoch_data = match fs::read_to_string(epoch_file).await {
        Ok(content) => serde_json::from_str::<Vec<Value>>(&content).unwrap_or_else(|_| vec![]),
        Err(_) => vec![], // If file doesn't exist, start fresh
    };

    // Append new unit entry
    epoch_data.push(unit_entry);

    // Open file and write updated content
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(epoch_file)
        .await?;

    file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes())
        .await?;
    info!("Successfully wrote finalized unit to file: {}", epoch_file);
    Ok(())
}