use std::{collections::HashMap, error::Error};
use tracing::{ error, info};
use reqwest::Client;
use base64::{engine::general_purpose, Engine};
use crate::{
    structs::node::Node,
    utils::merkle_utils::validate_merkle_branch,
};

/// Checks whether the local DAG is synchronized with the target node's DAG.
pub async fn check_dag_sync(
    client: &Client,
    epoch_id: u64,
    sender_id: &usize,
    sender_url: &String,
) -> Result<bool, Box<dyn Error>> {
    let url = format!("{}/dag_sync", sender_url);

    let payload = serde_json::json!({
        "epoch_id": epoch_id,
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
            "DAG sync status with sender {} for epoch {}: {}",
            sender_id, epoch_id, in_sync
        );
        Ok(in_sync)
    } else {
        error!(
            "Failed to check DAG sync with sender {} for epoch {}. Status: {}",
            sender_id, epoch_id, response.status()
        );
        Err(format!("Failed to check DAG sync. Status: {}", response.status()).into())
    }
}

/// Ensures that the DAG has reached the required round before progressing.
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
    info!(
        "Node {}: DAG is synchronized to round {} or beyond.",
        node.id, target_round - 1
    );
    Ok(())
}

/// Checks if the parents of a given unit are available in the local DAG.
pub async fn are_parents_available(node: &Node, unit: &[u8]) -> bool {
    info!("Node {}: Checking parent availability for unit", node.id);

    // Check if the DAG is empty
    let dag_read = node.dag.read().await;
    if dag_read.is_empty() {
        // Check if it is the first round
            info!(
                "Node {}: DAG is empty and this is the first transaction. Skipping parent validation.",
                node.id
            );
        return true; // Allow validation to pass
    }
    
    // Extract parent hashes
    let parent_hashes = match get_parent_hashes(unit) {
        Ok(hashes) => hashes,
        Err(e) => {
            error!("Node {}: Failed to extract parents from unit. Error: {:?}", node.id, e);
            return false;
        }
    };

    // Validate parent hashes against the DAG
    for parent in parent_hashes {
        if !dag_read.contains_key(&parent) {
            error!(
                "Node {}: Parent with hash {:?} is missing in the local DAG",
                node.id, parent
            );
            return false;
        }
    }

    info!("Node {}: All parents are locally available for unit", node.id);
    true
}







// pub async fn validate_unit_parents(node: &Node, unit_data: &[u8]) -> Result<(), String> {
//     // Check if DAG is empty or unpopulated
//     let dag = node.dag.read().await; // Access the DAG

//     info!(
//         "Node {}: DAG length is: {}",
//         node.id, dag.len()
//     );

//     if dag.is_empty() {
//         info!(
//             "Node {}: DAG is empty or not populated. Skipping parent validation.",
//             node.id
//         );
//         return Ok(()); // Allow parent validation to pass
//     }

//     // Extract all parent hashes at once
//     let parent_hashes = get_parent_hashes(unit_data)?;
//     info!("Node {}: Extracted parent hashes: {:?}", node.id, parent_hashes);

//     // Validate each parent hash against the DAG
//     for parent in &parent_hashes {
//         if !dag.contains_key(parent) {
//             return Err(format!(
//                 "Node {}: Parent unit {:?} not committed in DAG.",
//                 node.id, parent
//             ));
//         }
//     }

//     Ok(())
// }



pub async fn validate_unit_parents(node: &Node, unit_data: &[u8]) -> Result<(), String> {
    // Check if DAG is empty or unpopulated
    let dag = node.dag.read().await; // Access the DAG

    info!(
        "Node {}: DAG length is: {}",
        node.id, dag.len()
    );

    if dag.is_empty() {
        info!(
            "Node {}: DAG is empty because its first round proposal Skipping parent validation.",
            node.id
        );
        return Ok(())
    }

    // Extract all parent hashes at once
    let parent_hashes = get_parent_hashes(unit_data)?;
    info!("Node {}: Extracted parent hashes: {:?}", node.id, parent_hashes);

    // Validate each parent hash against the DAG
    for parent in &parent_hashes {
        if !dag.contains_key(parent) {
            return Err(format!(
                "Node {}: Parent unit {:?} not committed in DAG.",
                node.id, parent
            ));
        }
    }

    Ok(())
}


pub fn get_parent_hashes(unit: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    info!("Extracting parent hashes from unit: {:?}", unit);
    if unit.is_empty() {
        return Err("Unit is empty".to_string());
    }

    let parent_count = unit[0] as usize;
    let parent_size = 32; // Each parent hash is 32 bytes
    let parent_data_size = parent_count * parent_size;

    if unit.len() < 1 + parent_data_size {
        return Err("Unit data too short to contain all parent hashes".to_string());
    }

    let parents = unit[1..1 + parent_data_size]
        .chunks(parent_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    Ok(parents)
}


/// Ensures all parent units are committed in the DAG.
pub async fn ensure_all_parents_committed(
    node: &Node,
    parents: &[String], // Parent hashes in Base64 format
    root: &[u8],        // Root of the unit for error reporting
) -> Result<(), String> {
    for parent in parents {
        // Decode the parent ID from Base64
        let parent_bytes = match general_purpose::STANDARD.decode(parent) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Err(format!(
                    "Failed to decode parent ID {}: {:?}",
                    parent, e
                ));
            }
        };

        // Convert the single Vec<u8> into a slice of Vec<u8> for `is_unit_committed`
        if !node.is_unit_committed(&[parent_bytes]).await {
            return Err(format!(
                "Parent unit {} not committed for root {:?}",
                parent,
                general_purpose::STANDARD.encode(root), // Convert root to Base64 for readability
            ));
        }
    }
    Ok(())
}


/// Ensures DAG synchronization by validating epoch and DAG state.
pub async fn ensure_dag_synchronization(
    node: &Node,
    client: &Client,
    epoch_id: u64,
    sender_id: &usize,
    sender_url: &String,
) -> Result<(), String> {
    let _ = check_dag_sync(client, epoch_id, sender_id, sender_url).await;
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
    // Step 1: Validate Merkle branch
    if !validate_merkle_branch(shard_hashes, proofs, 0, root) {
        return Err(format!(
            "Node {}: Merkle branch validation failed for root {:?}",
            node.id, root
        ));
    }

    info!(
        "Node {}: Merkle branch validation passed for root {:?}",
        node.id, root
    );

    // Step 2: Check parent availability
    let parent_hash = &unit[unit.len() - 32..];
    {
        let dag_read = node.dag.read().await;
        if !dag_read.contains_key(parent_hash) {
            return Err(format!(
                "Node {}: Parent availability check failed for unit {:?}",
                node.id, unit
            ));
        }
    }

    info!(
        "Node {}: Parent availability check passed for unit {:?}",
        node.id, unit
    );

    Ok(())
}
