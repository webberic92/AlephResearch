use std::error::Error;

use tracing::{error,  info};
use reqwest::Client;

use crate::structs::{node::Node, toml_config::TomlConfig};

/// Checks whether the local DAG is synchronized with the target node's DAG.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `client`: HTTP client for making requests.
/// - `epoch_id`: The epoch ID for which to check synchronization.
/// - `target_node_id`: The ID of the target node.
///
/// # Returns
/// - `Result<bool, Box<dyn std::error::Error>>`: `Ok(true)` if synchronized, `Ok(false)` if not, or an error.

/// Checks if the DAG is in sync with the given node for the specified epoch.
///
/// Returns `Ok(true)` if in sync, `Ok(false)` if not in sync, and `Err` if an error occurs.
pub async fn check_dag_sync(client: &Client, epoch_id: u64, node_url: &str) -> Result<bool, Box<dyn Error>> {
    let url = format!("{}/check_dag_sync?epoch_id={}", node_url, epoch_id);
    info!("Checking DAG sync with {} for epoch {}", node_url, epoch_id);

    let response = client.get(&url).send().await?;

    if response.status().is_success() {
        let sync_status: bool = response.json().await?;
        Ok(sync_status)
    } else {
        Err(format!(
            "Failed to check DAG sync. Status: {}",
            response.status()
        ).into())
    }
}

/// Verifies whether the parents of the given unit are locally available in the DAG.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `unit`: The unit whose parent availability is being checked.
///
/// # Returns
/// - `true` if all parents are available.
/// - `false` if any parent is missing.
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

pub fn get_parents(unit: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    // Extract parent hashes from the unit
    // Mock implementation, replace with actual logic for your protocol
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


pub async fn ensure_dag_synchronization(
    client: &Client,
    epoch_id: u64,
    config: &TomlConfig, // Add config parameter to access network nodes
) -> Result<(), String> {
    for node_url in &config.network.nodes {
        if let Err(e) = check_dag_sync(client, epoch_id, &node_url, ).await {
            return Err(format!(
                "DAG synchronization failed with node {} for epoch {}: {:?}",
                node_url, epoch_id, e
            ));
        }
    }
    Ok(())
}




