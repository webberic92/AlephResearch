use tracing::info;

use crate::structs::node::Node;

pub async fn ensure_no_overlap(node: &Node, epoch_id: u64) -> Result<(), &'static str> {
    info!("Node {}: Detecting if there is overlap for epoch {}", node.id, epoch_id);

    let mut tracker = node.epoch_tracker.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Overlap for epoch {} detected, but continuing", node.id, epoch_id);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: No overlap detected for epoch {}", node.id, epoch_id);
        Ok(())
    }
}

pub async fn handle_sync_epoch(node: &Node, epoch_id: u64, sender: usize) -> Result<(), &'static str> {
    info!("Node {}: Synchronizing epoch {} from {}", node.id, epoch_id,sender);

    let mut tracker = node.epoch_tracker.lock().await;
    if tracker.contains(&epoch_id) {
        info!("Node {}: Epoch {} already synchronized with {}", node.id, epoch_id, sender);
        Ok(())
    } else {
        tracker.insert(epoch_id);
        info!("Node {}: Epoch {} synchronized successfully with node {}", node.id, epoch_id,sender);
        Ok(())
    }
}