use tracing::{error, info};

/// Logs a critical reconstruction failure and suggests recovery steps.
pub fn log_reconstruction_failure(node_id: usize, epoch_id: u64, error_message: &str) {
    error!(
        "Node {}: Reconstruction failed for epoch {}. Error: {}",
        node_id, epoch_id, error_message
    );
    info!(
        "Node {}: Attempting recovery for epoch {}. Ensure DAG synchronization and shard validity.",
        node_id, epoch_id
    );
}