use std::error::Error;
use serde_json::json;
use tracing::{debug, error, info};
use reqwest::Client;
use crate::{structs::node::Node, utils::merkle_utils::validate_merkle_branch};

/// Checks whether the local DAG is synchronized with the target node's DAG.
/// Logs request URL and payload, returning synchronization status.
pub async fn check_dag_sync(client: &Client, epoch_id: u64,sender_id: &usize, sender_url: &String) -> Result<bool, Box<dyn Error>> {
    let url = format!("{}/dag_sync", sender_url);
    let payload = json!({
        "epoch_id": epoch_id,
        "sender": sender_id // Include the target node URL in the payload
    });

    info!("Sending DAG sync check to URL: {} with payload: {:?}", url, payload);

    let response = client.post(&url).json(&payload).send().await?;
    info!("DAG sync response status: {}", response.status());

    if response.status().is_success() {
        let response_data: serde_json::Value = response.json().await?;
        let in_sync = response_data["in_sync"].as_bool().unwrap_or(false);
        info!("DAG sync status with {} for epoch {}: {}", sender_id, epoch_id, in_sync);
        Ok(in_sync)
    } else {
        error!("Failed to check DAG sync with {} for epoch {}. Status: {}", sender_id, epoch_id, response.status());
        Err(format!(
            "Failed to check DAG sync. Status: {}",
            response.status()
        ).into())
    }
}

/// Ensures that the DAG has reached the required round before progressing.
/// Adapts to use `node.epoch_round_id` since `node.get_current_round()` is unavailable.
pub async fn ensure_round_sync(node: &Node, target_round: u64) -> Result<(), String> {
    let current_round = {
        let epoch_round_id = node.epoch_round_id.lock().await;
        *epoch_round_id.iter().max().unwrap_or(&0)
    };

    if current_round < target_round - 1 {
        return Err(format!(
            "Node {}: DAG not synchronized to round {} for prevote (current round: {})",
            node.id, target_round - 1, current_round
        ));
    }
    info!("Node {}: DAG is synchronized to round {} or beyond.", node.id, target_round - 1);
    Ok(())
}

/// Validates that all parents of a unit are present in the local DAG.
pub async fn validate_unit_parents(node: &Node, unit: &[u8]) -> Result<(), String> {
    if !are_parents_available(node, unit).await {
        return Err(format!("Node {}: Missing parents for unit", node.id));
    }
    info!("Node {}: All parents for unit are available.", node.id);
    Ok(())
}

/// Checks if the parents of a given unit are available in the local DAG.
pub async fn are_parents_available(node: &Node, unit: &[u8]) -> bool {
    info!("Node {}: Checking parent availability for unit", node.id);

    // Retrieve the list of parent hashes for the given unit
    let parent_hashes = match get_parents(unit) {
        Ok(hashes) => hashes,
        Err(e) => {
            error!("Node {}: Failed to extract parents from unit. Error: {:?}", node.id, e);
            return false;
        }
    };

    // Check if each parent hash is present in the local DAG
    let dag_read = node.dag.read().await; // Assuming `dag` is a `RwLock`-protected HashMap
    for parent_hash in parent_hashes {
        if !dag_read.contains_key(&parent_hash) {
            error!(
                "Node {}: Parent with hash {:?} is missing in the local DAG",
                node.id, parent_hash
            );
            return false;
        }
    }

    info!("Node {}: All parents are locally available for unit", node.id);
    true
}

/// Mock implementation to extract parent hashes from the unit.
/// Replace with actual logic for your protocol.
pub fn get_parents(unit: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if unit.is_empty() {
        return Err("Unit is empty".to_string());
    }

    // Assuming parents are stored as concatenated hashes at the end of the unit
    let parent_count = 2; // Example: each unit has 2 parents
    let parent_size = 32; // Example: each hash is 32 bytes

    if unit.len() < parent_count * parent_size {
        return Err("Unit data is too small to contain parent hashes".to_string());
    }

    let mut parents = vec![];
    let start = unit.len() - (parent_count * parent_size);
    for i in 0..parent_count {
        let offset = start + (i * parent_size);
        parents.push(unit[offset..offset + parent_size].to_vec());
    }

    Ok(parents)
}

/// Ensures DAG synchronization by checking the current epoch and validating the DAG.
pub async fn ensure_dag_synchronization(
    node: &Node, // Pass node as an argument
    client: &Client,
    epoch_id: u64,
    sender_id: &usize,
    sender_url: &String,
) -> Result<(), String> {
    // Step 1: Check basic DAG synchronization with the target node
    if let Err(e) = check_dag_sync(client, epoch_id, sender_id, sender_url).await {
        return Err(format!(
            "DAG synchronization failed with node {} for epoch {}: {:?}",
            sender_id, epoch_id, e
        ));
    }

    // Step 2: Ensure the DAG has reached the required round for prevote
    ensure_round_sync(node, epoch_id).await?;

    Ok(())
}
pub async fn validate_unit(
    node: &Node,
    unit: &[u8],
    root: &[u8],
    shard_hashes: &[Vec<u8>],
    proofs: &[Vec<u8>],
) -> Result<(), String> {
    info!("Node {}: Validating unit with root {:?}", node.id, root);

    // Step 1: Validate Merkle branch
    if !validate_merkle_branch(shard_hashes, proofs, 0, root) {
        let error_message = format!(
            "Node {}: Merkle branch validation failed for root {:?}",
            node.id, root
        );
        error!("{}", error_message);
        return Err(error_message);
    }
    info!("Node {}: Merkle branch validation passed for root {:?}", node.id, root);

    // Step 2: Check parent availability
    if unit.len() < 32 {
        let error_message = format!(
            "Node {}: Unit length too short to extract parent hash: length = {}",
            node.id, unit.len()
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    let parent_hash = &unit[unit.len() - 32..];
    debug!(
        "Node {}: Extracted parent hash from unit: {:?}",
        node.id, parent_hash
    );

    let dag = node.dag.read().await;
    debug!("Node {}: DAG contents: {:?}", node.id, dag.keys().collect::<Vec<_>>());

    if !dag.contains_key(parent_hash) {
        let error_message = format!(
            "Node {}: Parent availability check failed for parent hash {:?}",
            node.id, parent_hash
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {}: Parent availability check passed for parent hash {:?}",
        node.id, parent_hash
    );

    Ok(())
}


