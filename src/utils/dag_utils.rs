// use std::error::Error;
// use tracing::{debug, error, info};
// use reqwest::Client;
// use crate::{structs::{node::Node, requests::DAGSyncRequest}, utils::merkle_utils::validate_merkle_branch};
// use base64::{engine::general_purpose, Engine};

// /// Checks whether the local DAG is synchronized with the target node's DAG.
// /// Logs request URL and payload, returning synchronization status.
// // pub async fn check_dag_sync(client: &Client, epoch_id: u64,sender_id: &usize, sender_url: &String) -> Result<bool, Box<dyn Error>> {
// //     let url = format!("{}/dag_sync", sender_url);
    

// //  // Create the payload using the DAGSyncRequest struct
// //     let payload = DAGSyncRequest {
// //         epoch_id,
// //         sender_id: *sender_id,
// //         sender_url: sender_url.clone(),
// //     };

// //     info!("Sending DAG sync check to URL: {} with payload: {:?}", url, payload);

// //     let response = client.post(&url).json(&payload).send().await?;
// //     info!("DAG sync response status: {}", response.status());

// //     if response.status().is_success() {
// //         let response_data: serde_json::Value = response.json().await?;
// //         let in_sync = response_data["in_sync"].as_bool().unwrap_or(false);
// //         info!("DAG sync status with {} for epoch {}: {}", sender_id, epoch_id, in_sync);
// //         Ok(in_sync)
// //     } else {
// //         error!("Failed to check DAG sync with {} for epoch {}. Status: {}", sender_id, epoch_id, response.status());
// //         Err(format!(
// //             "Failed to check DAG sync. Status: {}",
// //             response.status()
// //         ).into())
// //     }
// // }
// pub async fn check_dag_sync(
//     client: &Client,
//     epoch_id: u64,
//     sender_id: &usize,
//     sender_url: &String,
// ) -> Result<bool, Box<dyn Error>> {
//     let url = format!("{}/dag_sync", sender_url);

//     let payload = serde_json::json!({
//         "epoch_id": epoch_id,
//         "sender_id": *sender_id,
//         "sender_url": sender_url.clone()
//     });

//     info!("Sending DAG sync check to URL: {} with payload: {:?}", url, payload);

//     let response = client.post(&url).json(&payload).send().await?;
//     info!("DAG sync response status: {}", response.status());

//     if response.status().is_success() {
//         let response_data: serde_json::Value = response.json().await?;
//         let in_sync = response_data["in_sync"].as_bool().unwrap_or(false);
//         info!("DAG sync status with {} for epoch {}: {}", sender_id, epoch_id, in_sync);
//         Ok(in_sync)
//     } else {
//         error!(
//             "Failed to check DAG sync with {} for epoch {}. Status: {}",
//             sender_id, epoch_id, response.status()
//         );
//         Err(format!(
//             "Failed to check DAG sync. Status: {}",
//             response.status()
//         )
//         .into())
//     }
// }



// /// Ensures that the DAG has reached the required round before progressing.
// /// Adapts to use `node.epoch_round_id` since `node.get_current_round()` is unavailable.
// pub async fn ensure_round_sync(node: &Node, target_round: u64) -> Result<(), String> {
//     let current_round = {
//         let epoch_round_id = node.epoch_round_id.lock().await;
//         *epoch_round_id.iter().max().unwrap_or(&0)
//     };

//     if current_round < target_round - 1 {
//         return Err(format!(
//             "Node {}: DAG not synchronized to round {} for prevote (current round: {})",
//             node.id, target_round - 1, current_round
//         ));
//     }
//     info!("Node {}: DAG is synchronized to round {} or beyond.", node.id, target_round - 1);
//     Ok(())
// }


// // /// Checks if the parents of a given unit are available in the local DAG.
// pub async fn are_parents_available(node: &Node, unit: &[u8]) -> bool {
//     info!("Node {}: Checking parent availability for unit", node.id);

//     // Retrieve the list of parent hashes for the given unit
//     let parent_hashes = match get_parents(unit) {
//         Ok(hashes) => hashes,
//         Err(e) => {
//             error!("Node {}: Failed to extract parents from unit. Error: {:?}", node.id, e);
//             return false;
//         }
//     };

//     // Convert the flat `Vec<u8>` into a set of parent hashes (32-byte chunks)
//     const HASH_SIZE: usize = 32; // Assuming each parent hash is 32 bytes
//     if parent_hashes.len() % HASH_SIZE != 0 {
//         error!(
//             "Node {}: Invalid parent hashes length: {}",
//             node.id, parent_hashes.len()
//         );
//         return false;
//     }

//     let parents: Vec<Vec<u8>> = parent_hashes
//         .chunks(HASH_SIZE) // Split into 32-byte chunks
//         .map(|chunk| chunk.to_vec()) // Convert each chunk into a Vec<u8>
//         .collect();

//     // Check if each parent hash is present in the local DAG
//     let dag_read = node.dag.read().await; // Assuming `dag` is a `RwLock`-protected HashMap<Vec<u8>, Vec<u8>>
//     for parent_hash in &parents {
//         if !dag_read.contains_key(parent_hash) {
//             error!(
//                 "Node {}: Parent with hash {:?} is missing in the local DAG",
//                 node.id, parent_hash
//             );
//             return false;
//         }
//     }

//     info!("Node {}: All parents are locally available for unit", node.id);
//     true
// }





// pub async fn validate_unit_parents(node: &Node, unit: &[u8]) -> Result<(), String> {
//     // Extract parent hashes using `get_parents`
//     let parent_hashes = match get_parents(unit) {
//         Ok(hashes) => {
//             info!("Node {}: Extracted parent hashes: {:?}", node.id, hashes);
//             hashes
//         }
//         Err(e) => {
//             let error_message = format!(
//                 "Node {}: Failed to extract parent hashes from unit. Error: {}",
//                 node.id, e
//             );
//             error!("{}", error_message);
//             return Err(error_message);
//         }
//     };

//     // Convert parent hashes to Base64 strings
//     let parent_base64: Vec<String> = parent_hashes
//         .into_iter()
//         .map(|hash| base64::engine::general_purpose::STANDARD.encode(hash))
//         .collect();

//     // Ensure all parents are committed
//     ensure_all_parents_committed(node, &parent_base64, unit).await?;

//     info!(
//         "Node {}: All parents are available and committed for the unit.",
//         node.id
//     );
//     Ok(())
// }



// pub fn get_parents(unit: &[u8]) -> Result<Vec<u8>, String> {
//     if unit.is_empty() {
//         return Err("Unit is empty".to_string());
//     }

//     // Parse the unit to extract the parent count and parent hashes
//     if unit.len() < 1 {
//         return Err("Unit data too short to extract parent count".to_string());
//     }

//     // Step 1: Extract parent count
//     let parent_count = unit[0] as usize; // Assume the first byte represents the number of parents

//     // Step 2: Calculate the expected size of the parent hashes
//     let parent_size = 32; // Assume each parent hash is 32 bytes
//     let parent_data_size = parent_count * parent_size;

//     if unit.len() < 1 + parent_data_size {
//         return Err("Unit data too short to contain all parent hashes".to_string());
//     }

//     // Step 3: Extract parent hashes into a single flat Vec<u8>
//     let start = 1; // Parent data starts immediately after the parent count byte
//     let parents = unit[start..start + parent_data_size].to_vec();

//     Ok(parents)
// }


// /// Ensures DAG synchronization by checking the current epoch and validating the DAG.
// pub async fn ensure_dag_synchronization(
//     node: &Node, // Pass node as an argument
//     client: &Client,
//     epoch_id: u64,
//     sender_id: &usize,
//     sender_url: &String,
// ) -> Result<(), String> {
//     // Step 1: Check basic DAG synchronization with the target node
//     if let Err(e) = check_dag_sync(client, epoch_id, sender_id, sender_url).await {
//         return Err(format!(
//             "DAG synchronization failed with node {} for epoch {}: {:?}",
//             sender_id, epoch_id, e
//         ));
//     }

//     // Step 2: Ensure the DAG has reached the required round for prevote
//     ensure_round_sync(node, epoch_id).await?;

//     Ok(())
// }
// pub async fn validate_unit(
//     node: &Node,
//     unit: &[u8],
//     root: &[u8],
//     shard_hashes: &[Vec<u8>],
//     proofs: &[Vec<u8>],
// ) -> Result<(), String> {
//     info!("Node {}: Validating unit with root {:?}", node.id, root);

//     // Step 1: Validate Merkle branch
//     if !validate_merkle_branch(shard_hashes, proofs, 0, root) {
//         let error_message = format!(
//             "Node {}: Merkle branch validation failed for root {:?}",
//             node.id, root
//         );
//         error!("{}", error_message);
//         return Err(error_message);
//     }
//     info!("Node {}: Merkle branch validation passed for root {:?}", node.id, root);

//     // Step 2: Check parent availability
//     if unit.len() < 32 {
//         let error_message = format!(
//             "Node {}: Unit length too short to extract parent hash: length = {}",
//             node.id, unit.len()
//         );
//         error!("{}", error_message);
//         return Err(error_message);
//     }

//     let parent_hash = &unit[unit.len() - 32..];
//     debug!(
//         "Node {}: Extracted parent hash from unit: {:?}",
//         node.id, parent_hash
//     );

//     let dag = node.dag.read().await;
//     debug!("Node {}: DAG contents: {:?}", node.id, dag.keys().collect::<Vec<_>>());

//     if !dag.contains_key(parent_hash) {
//         let error_message = format!(
//             "Node {}: Parent availability check failed for parent hash {:?}",
//             node.id, parent_hash
//         );
//         error!("{}", error_message);
//         return Err(error_message);
//     }

//     info!(
//         "Node {}: Parent availability check passed for parent hash {:?}",
//         node.id, parent_hash
//     );

//     Ok(())
// }


// pub async fn ensure_all_parents_committed(
//     node: &Node,
//     parents: &[String], // Parent hashes in Base64 format
//     root: &[u8],
// ) -> Result<(), String> {
//     for parent in parents {
//         // Decode the parent ID from Base64
//         let parent_bytes = match base64::engine::general_purpose::STANDARD.decode(parent) {
//             Ok(bytes) => bytes,
//             Err(e) => {
//                 return Err(format!(
//                     "Failed to decode parent ID {}: {:?}",
//                     parent, e
//                 ));
//             }
//         };

//         // Check if the parent unit is committed
//         if !node.is_unit_committed(&parent_bytes).await {
//             return Err(format!(
//                 "Parent unit {} not committed for root {:?}",
//                 parent, // Use the original string for readable error messages
//                 base64::engine::general_purpose::STANDARD.encode(root), // Convert root to readable format
//             ));
//         }
//     }
//     Ok(())
// }

use std::error::Error;
use tracing::{debug, error, info};
use reqwest::Client;
use base64::{engine::general_purpose, Engine};
use crate::{
    structs::{node::Node, requests::DAGSyncRequest},
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

    let parent_hashes = match get_parents(unit) {
        Ok(hashes) => hashes,
        Err(e) => {
            error!("Node {}: Failed to extract parents from unit. Error: {:?}", node.id, e);
            return false;
        }
    };

    const HASH_SIZE: usize = 32; // Each parent hash is 32 bytes
    if parent_hashes.len() % HASH_SIZE != 0 {
        error!(
            "Node {}: Invalid parent hashes length: {}",
            node.id, parent_hashes.len()
        );
        return false;
    }

    let parents: Vec<Vec<u8>> = parent_hashes
        .chunks(HASH_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();

    let dag_read = node.dag.read().await;
    for parent_hash in &parents {
        if !dag_read.contains_key(parent_hash) {
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

/// Validates whether the parents of a unit are committed in the DAG.
pub async fn validate_unit_parents(node: &Node, unit: &[u8]) -> Result<(), String> {
    let parent_hashes = match get_parents(unit) {
        Ok(hashes) => {
            info!("Node {}: Extracted parent hashes: {:?}", node.id, hashes);
            hashes
        }
        Err(e) => {
            let error_message = format!(
                "Node {}: Failed to extract parent hashes from unit. Error: {}",
                node.id, e
            );
            error!("{}", error_message);
            return Err(error_message);
        }
    };

    let parent_base64: Vec<String> = parent_hashes
        .chunks(32)
        .map(|chunk| general_purpose::STANDARD.encode(chunk))
        .collect();

    ensure_all_parents_committed(node, &parent_base64, unit).await?;

    info!(
        "Node {}: All parents are available and committed for the unit.",
        node.id
    );
    Ok(())
}

/// Extracts parent hashes from a unit.
pub fn get_parents(unit: &[u8]) -> Result<Vec<u8>, String> {
    if unit.is_empty() {
        return Err("Unit is empty".to_string());
    }

    let parent_count = unit[0] as usize;
    let parent_size = 32;
    let parent_data_size = parent_count * parent_size;

    if unit.len() < 1 + parent_data_size {
        return Err("Unit data too short to contain all parent hashes".to_string());
    }

    let parents = unit[1..1 + parent_data_size].to_vec();
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
    check_dag_sync(client, epoch_id, sender_id, sender_url).await;
    ensure_round_sync(node, epoch_id).await?;
    Ok(())
}

/// Validates a unit by checking its Merkle branch and parent availability.
// pub async fn validate_unit(
//     node: &Node,
//     unit: &[u8],
//     root: &[u8],
//     shard_hashes: &[Vec<u8>],
//     proofs: &[Vec<u8>],
// ) -> Result<(), String> {
//     if !validate_merkle_branch(shard_hashes, proofs, 0, root) {
//         return Err(format!(
//             "Node {}: Merkle branch validation failed for root {:?}",
//             node.id, root
//         ));
//     }

//     if !are_parents_available(node, unit).await {
//         return Err(format!(
//             "Node {}: Parent availability check failed for unit {:?}",
//             node.id, unit
//         ));
//     }

//     Ok(())
// }
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
