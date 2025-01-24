use std::path::Path;
use std::sync::Arc;
use reqwest::Client;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use serde_json::json;
use tracing::{error, info};
use crate::requests::ip_server_requests::notify_transaction_submitted;
use crate::structs::node::Node;
use crate::structs::toml_config;
use crate::utils::config_util::load_config;
use crate::utils::dag_utils::{are_parents_available, validate_unit};
use crate::utils::merkle_utils::validate_merkle_branch;

/// Handles the commit phase in the Aleph protocol based on the ch-RBC proof.
///
/// This function finalizes and persists the reconstructed unit if the quorum threshold is reached,
/// after validating the Merkle proof and ensuring the availability of parent nodes.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `sender`: ID of the node sending the commit request.
/// - `root`: The Merkle tree root associated with the commit.
/// - `unit`: The reconstructed unit.
/// - `epoch_id`: The epoch in which the commit is being processed.
/// - `shard_hashes`: Hashes of the original shards used to construct the Merkle tree.
/// - `proofs`: Merkle proofs used for validation.
pub async fn handle_commit(
    node: &Node,
    client: Arc<Client>,
    sender: usize,
    root: Vec<u8>,
    unit: Vec<u8>,
    epoch_id: u64,
    shard_hashes: Vec<Vec<u8>>,
    proofs: Vec<Vec<u8>>,
) -> Result<(), String> {
    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node.id, sender, epoch_id
    );

    // Step 1: Validate the reconstructed unit with the Merkle root
    info!("Node {}: Validating unit for root {:?}", node.id, root);
    if let Err(e) = validate_unit(node, &unit, &root, &shard_hashes, &proofs).await {
        error!(
            "Node {}: Validation failed for unit with root {:?}: {}",
            node.id, root, e
        );
        return Err(e);
    }
    info!("Node {}: Unit validation passed for root {:?}", node.id, root);

    // Step 2: Check if parents of the unit are available
    info!("Node {}: Checking parent availability for the unit.", node.id);
    if !are_parents_available(node, &unit).await {
        let error_message = format!(
            "Node {}: Parent availability check failed for unit associated with root {:?}",
            node.id, root
        );
        error!("{}", error_message);
        return Err(error_message);
    }
    info!("Node {}: Parent availability check passed.", node.id);

    // Step 3: Persist the finalized unit to storage
    let epoch_dir = "./finalized_units";
    let epoch_file = format!("{}/epoch{}.json", epoch_dir, epoch_id);
    info!("Node {}: Persisting finalized unit to file: {}", node.id, epoch_file);

    if let Err(e) = fs::create_dir_all(Path::new(epoch_dir)).await {
        let error_message = format!(
            "Node {}: Failed to create directory for finalized units: {:?}",
            node.id, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    if let Err(e) = append_finalized_unit(&epoch_file, node.id, sender, root.clone(), unit).await {
        let error_message = format!(
            "Node {}: Failed to append finalized unit to file {}: {:?}",
            node.id, epoch_file, e
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    info!(
        "Node {}: Successfully finalized unit for root {:?} and persisted it to file.",
        node.id, root
    );
    let toml_config = load_config();
    notify_transaction_submitted(&client, &toml_config).await;

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
