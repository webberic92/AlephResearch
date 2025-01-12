use crate::utils::errors_util::log_reconstruction_failure;
use crate::utils::recovery_util::attempt_recovery;
use crate::utils::dag_utils::ensure_dag_synchronization; // Ensure DAG synchronization
use crate::{handlers::handle_commit::handle_commit, utils::merkle_utils::reconstruct_unit};
use crate::structs::node::Node;
use crate::utils::merkle_utils::validate_merkle_branch;
use reqwest::Client;
use tracing::{error, info, debug};

/// Handles a prevote request in the Aleph protocol.
///
/// This function tracks quorum votes for a given root and triggers the commit phase if quorum is reached.
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

    // Validate Merkle Branch
    info!("Node {}: Validating Merkle branch for shard", node.id);
    let computed_root = validate_merkle_branch(&shard, &proof);
    if computed_root != root {
        error!(
            "Node {}: Prevote phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
        return;
    }
    info!("Node {}: Merkle branch validation passed", node.id);

    // Ensure DAG synchronization for each node in the network
        info!("Node {}: Ensuring DAG synchronization with {}", node.id, node_url);
        if let Err(e) = ensure_dag_synchronization(client, epoch_id, node_url).await {
            error!(
                "Node {}: DAG synchronization failed with node {}. Error: {:?}",
                node.id, node_url, e
            );
            return;
        }
        info!("Node {}: DAG synchronization successful with {}", node.id, node_url);
    

    // Update quorum votes
    let  quorum_votes = node.quorum_votes.write().await;
    let mut quorum_votes_clone = quorum_votes.clone();
    let count = quorum_votes_clone.entry(root.clone()).or_insert(0);
    *count += 1;
    debug!("Node {}: Updated quorum votes: {:?}", node.id, quorum_votes);

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
                error!("Node {}: Reconstruction failed for root {:?}. Error: {:?}", node.id, root, e);
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
    info!("Node {}: Finished handling PREVOTE REQUEST from Node {}", node.id, sender);
}
