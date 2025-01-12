use crate::structs::toml_config::TomlConfig;
use crate::utils::config_util::load_config;
use crate::utils::errors_util::log_reconstruction_failure;
use crate::utils::recovery_util::attempt_recovery;
use crate::utils::dag_utils::ensure_dag_synchronization; // Ensure DAG synchronization
use crate::{handlers::handle_commit::handle_commit, utils::merkle_utils::reconstruct_unit};
use crate::structs::node::Node;
use crate::utils::merkle_utils::validate_merkle_branch;
use reqwest::Client;
use tracing::{error, info};

/// Handles a prevote request in the Aleph protocol.
///
/// This function tracks quorum votes for a given root and triggers the commit phase if quorum is reached.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `client`: HTTP client for sending recovery requests.
/// - `config`: TOML configuration containing network information.
/// - `sender`: ID of the node sending the prevote request.
/// - `root`: The Merkle tree root associated with the prevote.
/// - `proof`: The Merkle proof for the shard.
/// - `shard`: The shard data.
/// - `epoch_id`: The epoch in which the prevote is being processed.
/// - `node_url`: URL of the node for DAG synchronization and recovery.
pub async fn handle_prevote(
    node: &Node,
    client: &Client,
    sender: usize,
    root: Vec<u8>,
    proof: Vec<Vec<u8>>,
    shard: Vec<u8>,
    epoch_id: u64,
    node_url: &String,
) {
    info!("Node {}: ==== Handling PREVOTE REQUEST from Node {} ====", node.id, sender);

        // Load configuration
    let config_path = "/home/aleph-node/aleph-node-config.toml"; // Adjust path as needed
    let config = load_config(config_path);
    
    // Validate Merkle Branch
    let computed_root = validate_merkle_branch(&shard, &proof);
    if computed_root != root {
        error!(
            "Node {}: Prevote phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
        return;
    }

    // Ensure DAG synchronization
    if let Err(e) = ensure_dag_synchronization(client, epoch_id, &config).await {
        error!("Node {}: DAG synchronization failed. Error: {:?}", node.id, e);
        return;
    }

    // Update quorum votes
    let mut quorum_votes = node.quorum_votes.write().await;
    let count = quorum_votes.entry(root.clone()).or_insert(0);
    *count += 1;

    if *count >= node.get_quorum_threshold() {
        info!(
            "Node {}: Quorum reached for root {:?} with {} votes",
            node.id, root, *count
        );

        // Reconstruction and Validation Logic
        info!(
            "Node {}: Starting reconstruction for unit associated with root {:?}",
            node.id, root
        );

        match reconstruct_unit(&[shard.to_vec()], &proof) {
            Ok(reconstructed_unit) => {
                info!(
                    "Node {}: Reconstruction successful for root {:?}. Proceeding to commit.",
                    node.id, root
                );

                // Handle commit phase
                handle_commit(node, sender, root, reconstructed_unit, epoch_id).await;
            }
            Err(e) => {
                log_reconstruction_failure(node.id, epoch_id, &e);
                if let Err(recovery_err) = attempt_recovery(node, client, epoch_id, node_url).await {
                    error!(
                        "Node {}: Recovery failed for epoch {}. Error: {}",
                        node.id, epoch_id, recovery_err
                    );
                }
            }
        }
    } else {
        info!(
            "Node {}: Prevote accepted for root {:?}. Current votes: {}",
            node.id, root, *count
        );
    }
}

