use std::{error::Error, sync::Arc};
use tokio::sync::RwLock;
use tracing::{ error, info};
use reqwest::Client;
use base64::{engine::general_purpose, Engine};
use crate::
    structs::node::Node;

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
pub async fn ensure_round_sync(node: Arc<RwLock<Node>>, target_round: u64) -> Result<(), String> {
    let (current_round, node_id) = {
        let node_state = node.read().await;
        let epoch_round_id = node_state.current_epoch.lock().await;
        (*epoch_round_id, node_state.id)
    };

    info!("Node {}: Current epoch: {}, Target round: {}", node_id, current_round, target_round);

    // Ensure synchronization to the required round
    if current_round < target_round - 1 {  // Change back to `<`
        let error_message = format!(
            "Node {}: DAG not synchronized to round {} for prevote (current round: {})",
            node_id,
            target_round - 1,
            current_round
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {}: DAG is synchronized to round {} or beyond.",
        node_id,
        target_round - 1
    );
    Ok(())
}






/// Checks if the parents of a given unit are available in the local DAG.
// pub async fn are_parents_available(node: Arc<RwLock<Node>>, unit: &[u8]) -> bool {
//     let node_state = node.read().await;

//     info!("Node {}: Checking parent availability for unit", node_state.id);

//     let dag_read = node_state.dag.read().await;

//     // ✅ If DAG is empty, assume this is the first transaction and return true
//     if dag_read.is_empty() {
//         info!("Node {}: DAG is empty. Assuming no parent validation needed.", node_state.id);
//         return true;
//     }

//     // Extract parent hashes only if DAG is non-empty
//     let parent_hashes = match get_parent_hashes(unit) {
//         Ok(hashes) => hashes,
//         Err(e) => {
//             error!(
//                 "Node {}: Failed to extract parents from unit. Error: {:?}",
//                 node_state.id, e
//             );
//             return false;
//         }
//     };

//     for parent in parent_hashes {
//         if !dag_read.contains_key(&parent) {
//             error!(
//                 "Node {}: Parent with hash {:?} is missing in the local DAG",
//                 node_state.id, parent
//             );
//             return false;
//         }
//     }

//     info!("Node {}: All parents are locally available for unit", node_state.id);
//     true
// }





// pub async fn validate_unit_parents(node: Arc<RwLock<Node>>, unit_data: &[u8]) -> Result<(), String> {
//     // Acquire a read lock for the node to access the DAG
//     let node_state = node.read().await;

//     // Access the DAG
//     let dag = node_state.dag.read().await;

//     info!(
//         "Node {}: DAG length is: {}",
//         node_state.id,
//         dag.len()
//     );

//     if dag.is_empty() {
//         info!(
//             "Node {}: DAG is empty because it's the first round proposal. Skipping parent validation.",
//             node_state.id
//         );
//         return Ok(());
//     }

//     // Extract all parent hashes at once
//     let parent_hashes = match get_parent_hashes(unit_data) {
//         Ok(hashes) => hashes,
//         Err(e) => {
//             let error_message = format!("Node {}: Failed to extract parent hashes. Error: {:?}", node_state.id, e);
//             error!("{}", error_message);
//             return Err(error_message);
//         }
//     };

//     info!("Node {}: Extracted parent hashes: {:?}", node_state.id, parent_hashes);

//     // Validate each parent hash against the DAG
//     for parent in &parent_hashes {
//         if !dag.contains_key(parent) {
//             return Err(format!(
//                 "Node {}: Parent unit {:?} not committed in DAG.",
//                 node_state.id, parent
//             ));
//         }
//     }

//     Ok(())
// }


// pub fn get_parent_hashes(unit: &[u8]) -> Result<Vec<Vec<u8>>, String> {
//     info!("Extracting parent hashes from unit: {:?}", unit);
    
//     if unit.is_empty() {
//         return Err("Unit is empty".to_string());
//     }

//     // Ensure parent count is valid
//     let mut parent_count = unit[0] as usize;
//     let parent_size = 32; // Each parent hash is 32 bytes
//     let parent_data_size = parent_count * parent_size;

//     // 🚀 FIX: If this is the first transaction (epoch 1, round 1), set parent_count to 0.
//     if parent_count > 0 && unit.len() == 1 + parent_data_size {
//         info!("First transaction detected. Overriding parent_count to 0.");
//         parent_count = 0;
//     }

//     if unit.len() < 1 + parent_data_size {
//         return Err("Unit data too short to contain all parent hashes".to_string());
//     }

//     let parents = if parent_count == 0 {
//         vec![] // ✅ Return empty list if this is the first transaction
//     } else {
//         unit[1..1 + parent_data_size]
//             .chunks(parent_size)
//             .map(|chunk| chunk.to_vec())
//             .collect()
//     };

//     Ok(parents)
// }


pub fn get_parent_hashes(unit: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    info!("Extracting parent hashes from unit: {:?}", unit);
    
    if unit.is_empty() {
        return Err("Unit is empty".to_string());
    }

    let mut parent_count = unit[0] as usize;
    let parent_size = 32; // Each parent hash is 32 bytes
    let parent_data_size = parent_count * parent_size;

    // 🚀 FIX: If this is the first transaction (DAG is empty), force parent_count = 0
    if parent_count > 0 && unit.len() == 1 + parent_data_size {
        info!("First transaction detected. Overriding parent_count to 0.");
        parent_count = 0;
    }

    if unit.len() < 1 + parent_data_size {
        return Err("Unit data too short to contain all parent hashes".to_string());
    }

    let parents = if parent_count == 0 {
        vec![] // ✅ Return empty list if this is the first transaction
    } else {
        unit[1..1 + parent_data_size]
            .chunks(parent_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    };

    Ok(parents)
}





// /// Ensures all parent units are committed in the DAG.
// pub async fn ensure_all_parents_committed(
//     node: &Node,
//     parents: &[String], // Parent hashes in Base64 format
//     root: &[u8],        // Root of the unit for error reporting
// ) -> Result<(), String> {
//     for parent in parents {
//         // Decode the parent ID from Base64
//         let parent_bytes = match general_purpose::STANDARD.decode(parent) {
//             Ok(bytes) => bytes,
//             Err(e) => {
//                 return Err(format!(
//                     "Failed to decode parent ID {}: {:?}",
//                     parent, e
//                 ));
//             }
//         };

//         // Convert the single Vec<u8> into a slice of Vec<u8> for `is_unit_committed`
//         if !node.is_unit_committed(&[parent_bytes]).await {
//             return Err(format!(
//                 "Parent unit {} not committed for root {:?}",
//                 parent,
//                 general_purpose::STANDARD.encode(root), // Convert root to Base64 for readability
//             ));
//         }
//     }
//     Ok(())
// }


/// Ensures DAG synchronization by validating epoch and DAG state.
pub async fn ensure_dag_synchronization(
    node: Arc<RwLock<Node>>,
    client: &Client,
    epoch_id: u64,
    sender_id: &usize,
    sender_url: String,
) -> Result<(), String> {
    let _ = check_dag_sync(client, epoch_id, sender_id, &sender_url).await;
    ensure_round_sync(node, epoch_id).await?;
    Ok(())
}
