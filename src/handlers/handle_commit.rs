use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use serde_json::json;
use tracing::{error, info};
use crate::structs::node::Node;
use crate::utils::merkle_utils::validate_merkle_branch;
use crate::utils::dag_utils::are_parents_available;

/// Handles the commit phase in the Aleph protocol with enhanced validation.
///
/// This function finalizes and persists the reconstructed unit if the quorum threshold is reached,
/// after validating the root and ensuring parents' availability.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `sender`: ID of the node sending the commit request.
/// - `root`: The Merkle tree root associated with the commit.
/// - `unit`: The finalized unit (block or transaction data).
/// - `epoch_id`: The epoch in which the commit is being processed.
pub async fn handle_commit(node: &Node, sender: usize, root: Vec<u8>, unit: Vec<u8>, epoch_id: u64) {
    info!("Node {}: ==Handling== commit request from Node {}", node.id, sender);

    let quorum_votes = node.quorum_votes.read().await;
    if let Some(counter) = quorum_votes.get(&root) {
        let quorum_threshold = node.get_quorum_threshold(); // 2f + 1
        if *counter >= quorum_threshold {
            info!(
                "Node {}: Quorum reached. Validating and committing for root {:?} with {} votes",
                node.id, root, *counter
            );

            // Validate h′=hh′
            if !validate_merkle_branch(&unit, &[root.clone()]).is_empty() {
                error!("Node {}: Merkle branch validation failed for root {:?}", node.id, root);
                return;
            }

            // Ensure parents of unit are available
            if !are_parents_available(node, &unit).await {
                error!(
                    "Node {}: Parent availability check failed for unit associated with root {:?}",
                    node.id, root
                );
                return;
            }

            // Persist the finalized unit
            let epoch_file = format!("./finalized_units/epoch{}.json", epoch_id);

            // Ensure the directory exists
            if let Err(e) = fs::create_dir_all("./finalized_units").await {
                error!("Node {}: Failed to create directory for finalized units: {:?}", node.id, e);
                return;
            }

            // Initialize or append to the epoch file
            if let Err(e) = append_finalized_unit(&epoch_file, node.id, sender, root, unit).await {
                error!("Node {}: Failed to append finalized unit to file {}: {:?}", node.id, epoch_file, e);
            } else {
                info!("Node {}: Successfully appended finalized unit to {}", node.id, epoch_file);
            }
        } else {
            info!(
                "Node {}: Insufficient votes for commit on root {:?} (current: {}, required: {})",
                node.id, root, *counter, quorum_threshold
            );
        }
    } else {
        info!("Node {}: Commit for unknown root {:?}", node.id, root);
    }
}

/// Appends a finalized unit to the epoch file.
///
/// # Arguments
/// - `epoch_file`: Path to the file where the unit will be stored.
/// - `node_id`: ID of the current node.
/// - `sender`: ID of the node that proposed the unit.
/// - `root`: The Merkle tree root associated with the unit.
/// - `unit`: The reconstructed and finalized unit.
async fn append_finalized_unit(
    epoch_file: &str,
    node_id: usize,
    sender: usize,
    root: Vec<u8>,
    unit: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read existing data or start with an empty JSON array
    let mut epoch_data = match fs::read_to_string(epoch_file).await {
        Ok(content) => serde_json::from_str::<Vec<serde_json::Value>>(&content).unwrap_or_else(|_| vec![]),
        Err(_) => vec![],
    };

    // Create a new finalized unit entry
    let unit_entry = json!({
        "node_id": node_id,
        "sender": sender,
        "root": root,
        "unit": unit,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    // Append the new entry
    epoch_data.push(unit_entry);

    // Write back the updated data
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true) // Overwrite the file
        .open(epoch_file)
        .await?;
    file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes())
        .await?;

    Ok(())
}
