use tracing::{error, info, warn};


use std::sync::Arc;
use tokio::sync::RwLock;

use crate::structs::node::Node;




/// Handles an incoming sync epoch request.
pub async fn handle_sync_epoch(node: Arc<RwLock<Node>>, sender_epoch: u64) -> Result<(), String> {
    // Step 1: Read the current epoch and drop the lock early
    let current_round = {
        let node_read = node.read().await;
        let epoch_guard = node_read.current_round.lock().await;
        *epoch_guard
    }; // 🔴 Drop both read locks before proceeding

    // Case 1: Out-of-sequence update (skipping epochs)
    if sender_epoch > current_round + 1 {
        let error_message = format!(
            "Node received an out-of-order epoch update. Expected: {} or {}, but received: {}.",
            current_round, current_round + 1, sender_epoch
        );
        warn!("{}", error_message);
        return Err(error_message);
    }

    // Case 2: Already up-to-date
    if sender_epoch == current_round {
        info!(
            "Node: Received epoch sync request for epoch {}, but already at the correct epoch.",
            sender_epoch
        );
        return Ok(());
    }

    // Step 2: Acquire write lock only when an update is needed
    {
        let mut node_write = node.write().await;
        let mut epoch_guard = node_write.current_round.lock().await;
        info!(
            "Node: Received valid epoch sync request. Advancing from epoch {} → epoch {}.",
            *epoch_guard, sender_epoch
        );
        *epoch_guard = sender_epoch;


    } // 🔴 Drop write lock immediately

    info!("Node: Successfully updated to epoch {}.", sender_epoch);
    Ok(())
}




