use crate::{handlers::handle_commit::handle_commit, structs::node::Node};
use tracing::info;

pub async fn handle_prevote(node: &Node, sender: usize, root: Vec<u8>, epoch_id: u64) {
    info!("Node {}: ==Handling== prevote request from Node {} : epoch {}", node.id, sender, epoch_id);

    let mut quorum_votes = node.quorum_votes.write().await;
    let counter = quorum_votes.entry(root.clone()).or_insert(0);
    *counter += 1;

    if *counter >= 2 * node.fault_tolerance_threshold() + 1 {
        info!(
            "Node {}: Quorum reached for root {:?} with {} votes",
            node.id, root, *counter
        );
        handle_commit(&node, sender, root).await;
    } else {
        info!(
            "Node {}: Prevote accepted for root {:?}, current votes: {}",
            node.id, root, *counter
        );
    }
    info!("Node {}: LEAVING prevote request from Node {}", node.id, sender);
}