use tracing::{error, info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::structs::node::Node;

/// Handles an incoming sync round request.
pub async fn handle_sync_round(node: Arc<Mutex<Node>>, sender_round: u64) -> Result<(), String> {
        info!(" ================= Handling sync round request  =================");
    // Step 1: Read the current round and drop the lock early
    let current_round = {
        let node_guard = node.lock().await;
        let round_guard = node_guard.current_round.lock().await;
        *round_guard
    }; // 🔴 Drop both locks here

    // Case 1: Out-of-sequence update (skipping rounds)
    if sender_round > current_round + 1 {
        let error_message = format!(
            "Node received an out-of-order round update. Expected: {} or {}, but received: {}.",
            current_round, current_round + 1, sender_round
        );
        warn!("{}", error_message);
        return Err(error_message);
    }

    // Case 2: Already up-to-date
    if sender_round == current_round {
        info!(
            "Node: Received round sync request for round {}, but already at the correct round.",
            sender_round
        );
        return Ok(());
    }

    // Step 2: Acquire lock only when an update is needed
    {
        let mut node_guard = node.lock().await;
        let mut round_guard = node_guard.current_round.lock().await;
        info!(
            "Node: Received valid round sync request. Advancing from round {} → round {}.",
            *round_guard, sender_round
        );
        *round_guard = sender_round;

    } // 🔴 Drop lock immediately

    info!("Node: Successfully updated to round {}.", sender_round);
    Ok(())
}
