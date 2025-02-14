use std::{error::Error, sync::Arc, usize};
use tokio::sync::RwLock;
use tracing::{ error, info};
use reqwest::Client;
use base64::{engine::general_purpose, Engine};
use crate::
    structs::node::Node;

/// Checks whether the local DAG is synchronized with the target node's DAG.
pub async fn check_dag_sync(
    client: &Client,
    round_id: u64,
    sender_id: &usize,
    sender_url: &String,
) -> Result<bool, Box<dyn Error>> {
    let url = format!("{}/dag_sync", sender_url);

    let payload = serde_json::json!({
        "round_id": round_id,
        "sender_id": *sender_id,
        "sender_url": sender_url.clone()
    });

    info!("Sending DAG sync check to URL: {} with payload: {:?}", url, payload);

    let response = client.post(&url).json(&payload).send().await?;
    info!("DAG sync response status: {}", response.status());

    if response.status().is_success() {
        let response_data: serde_json::Value = response.json().await?;
        let in_sync = response_data["in_sync"].as_bool().unwrap_or(false);
        info!(
            "DAG sync status with sender {} for round {}: {}",
            sender_id, round_id, in_sync
        );
        Ok(in_sync)
    } else {
        error!(
            "Failed to check DAG sync with sender {} for round {}. Status: {}",
            sender_id, round_id, response.status()
        );
        Err(format!("Failed to check DAG sync. Status: {}", response.status()).into())
    }
}






/// Ensures that the DAG has reached the required round before progressing.
pub async fn ensure_dag_round_sync(node: Arc<RwLock<Node>>, target_round: u64) -> Result<(), String> {
    let (latest_round, node_id, dag_keys) = {
        let node_state = node.read().await;
        let dag_read = node_state.dag.read().await;

        // 🔹 Clone the DAG keys instead of holding the lock
        let dag_keys: Vec<u64> = dag_read.keys().copied().collect();

        let latest_round = dag_keys.iter().max().copied().unwrap_or(1); // Default to 1 if empty
        (latest_round, node_state.id, dag_keys)
    }; // ✅ Drop read lock ASAP

    info!(
        "Node {}: DAG latest round: {}, Target round: {}",
        node_id, latest_round, target_round
    );

    if target_round == 1 {
        info!(
            "Node {}: First round detected (round 1). Skipping DAG sync check.",
            node_id
        );
        return Ok(());
    }

    if latest_round < target_round - 1 {
        let error_message = format!(
            "Node {}: DAG not synchronized. Latest round in DAG: {}, required: {}.",
            node_id, latest_round, target_round - 1
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {}: DAG is synchronized for round {} or beyond.",
        node_id, target_round - 1
    );
    
    Ok(())
}

/// Validates that the size of the received shards does not exceed the allowed limit.
/// Ensures compliance with ch-RBC constraints (max 256 bytes).
///
/// - Returns `true` if the size is valid.
/// - Returns `false` if the unit size exceeds 256 bytes.
// pub fn check_size(shards: &[Vec<u8>]) -> bool {
//     // ✅ Define the maximum allowed size per unit (256 bytes per ch-RBC spec)
//     const MAX_UNIT_SIZE: usize = 256;

//     // Compute total size of the unit by summing all shard sizes
//     let total_size: usize = shards.iter().map(|shard| shard.len()).sum();

//     // Log the computed size for debugging
//     info!("Checking unit size: {} bytes (Max allowed: {} bytes)", total_size, MAX_UNIT_SIZE);

//     // Return whether the size is within the allowed limit
//     total_size <= MAX_UNIT_SIZE
// }

pub fn check_size(shards: &[Vec<u8>], number_of_transactions: usize, transaction_size:usize) -> bool {
    let batch_size_limit = number_of_transactions * transaction_size;  // ✅ Dynamic limit

    let total_size: usize = shards.iter().map(|s| s.len()).sum();  // ✅ Sum up all shard sizes

    if total_size > batch_size_limit {
        info!(
            "Shard batch too large! Size: {} bytes (Max: {} bytes from config)",
            total_size, batch_size_limit
        );
        return false;
    }
    true
}

