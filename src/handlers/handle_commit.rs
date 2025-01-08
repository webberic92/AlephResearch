use tracing::info;
use crate::structs::node::Node;

pub async fn handle_commit(node: &Node, sender: usize, root: Vec<u8>) {
    info!("Node {}: ==Handling== commit request from Node {}", node.id, sender);

    let quorum_votes = node.quorum_votes.read().await;
    if let Some(counter) = quorum_votes.get(&root) {
        if *counter >= 2 * node.fault_tolerance_threshold() + 1 {
            info!("Node {}: Commit finalized for root {:?}", node.id, root);
        } else {
            info!(
                "Node {}: Insufficient votes for commit on root {:?}",
                node.id, root
            );
        }
    } else {
        info!("Node {}: Commit for unknown root {:?}", node.id, root);
    }
}
