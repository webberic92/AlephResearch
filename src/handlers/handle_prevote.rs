use crate::handlers::handle_commit::handle_commit; // Import the function, not the module
use crate::structs::node::Node;
use crate::utils::merkle_utils::validate_merkle_branch;
use tracing::{error, info};

/// Handles a prevote request in the Aleph protocol.
///
/// This function tracks quorum votes for a given root and triggers the commit phase if quorum is reached.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `sender`: ID of the node sending the prevote request.
/// - `root`: The Merkle tree root associated with the prevote.
/// - `proof`: The Merkle proof for the shard.
/// - `shard`: The shard data.
/// - `epoch_id`: The epoch in which the prevote is being processed.
pub async fn handle_prevote(
    node: &Node,
    sender: usize,
    root: Vec<u8>,
    proof: Vec<Vec<u8>>,
    shard: Vec<u8>,
    epoch_id: u64,
) {
    info!("Node {}: ==== Handling PREVOTE REQUEST from Node {} ====", node.id, sender);

    // Validate Merkle Branch
    let computed_root = validate_merkle_branch(&shard, &proof);
    if computed_root != root {
        error!(
            "Node {}: Prevote phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
            node.id, epoch_id, computed_root, root
        );
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

        // TODO: Implement actual reconstruction logic here
        // Ensure shards are correctly reconstructed into unit UU
        // Validate parent availability and correctness
        let reconstructed_unit = shard.clone(); // Placeholder: Replace with actual reconstruction logic
        info!(
            "Node {}: Reconstruction successful for root {:?}. Proceeding to commit.",
            node.id, root
        );

        // Handle commit phase
        handle_commit(node, sender, root, reconstructed_unit, epoch_id).await;
    } else {
        info!(
            "Node {}: Prevote accepted for root {:?}. Current votes: {}",
            node.id, root, *count
        );
    }
}
