use crate::handlers::handle_commit::handle_commit; // Import the function, not the module
use crate::structs::node::Node;
use tracing::info;

/// Handles a prevote request in the Aleph protocol.
///
/// This function tracks quorum votes for a given root and triggers the commit phase if quorum is reached.
///
/// # Arguments
/// - `node`: Reference to the current node.
/// - `sender`: ID of the node sending the prevote request.
/// - `root`: The Merkle tree root associated with the prevote.
/// - `epoch_id`: The epoch in which the prevote is being processed.
/// - `unit`: The finalized unit (block or transaction data) being committed if quorum is achieved.
pub async fn handle_prevote(node: &Node, sender: usize, root: Vec<u8>, epoch_id: u64, unit: Vec<u8>) {
    info!(
        "Node {}: ***==HANDLING PREVOTE REQUEST ==*** prevote request from Node {} : epoch {}",
        node.id, sender, epoch_id
    );

    // Update quorum votes for the given root
    let mut quorum_votes = node.quorum_votes.write().await;
    let counter = quorum_votes.entry(root.clone()).or_insert(0);
    *counter += 1;

    // Check if quorum threshold is reached
    if *counter >= node.get_quorum_threshold() {
        info!(
            "Node {}: Quorum reached for root {:?} with {} votes",
            node.id, root, *counter
        );

        // Call handle_commit with 5 arguments
        handle_commit(node, sender, root, unit, epoch_id).await;
    } else {
        info!(
            "Node {}: Prevote accepted for root {:?}, current votes: {}",
            node.id, root, *counter
        );
    }

    info!("Node {}: LEAVING prevote request from Node {}", node.id, sender);
}