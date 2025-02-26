use std::{ sync::Arc, usize};
use tokio::sync::Mutex;
use tracing::{ error, info};
use crate::
    structs::node::Node;

/// Checks whether the local DAG is synchronized with the target node's DAG.



/// Ensures that the DAG has reached the required round before progressing.
pub async fn ensure_dag_round_sync(node: Arc<Mutex<Node>>, current_round: u64) -> Result<(), String> {
    let node_id = {
        //info!("🔍 [DEBUG] Waiting to acquire node lock for round dag utils");
let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for round dag utils");
        node_guard.id
    };
    info!("Node {}: Ensuring DAG synchronization for round {}...", node_id, current_round);    let (latest_round, node_id, dag_keys) = {
        // 🔒 Lock the node only as long as necessary
        //info!("🔍 [DEBUG] Waiting to acquire node lock for round dag utils");
let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for round dag utils");
        let dag = node_guard.dag.lock().await;

        // Clone the DAG keys instead of holding the lock
        let dag_keys: Vec<u64> = dag.keys().copied().collect();
        let latest_round = dag_keys.iter().max().copied().unwrap_or(0);
        (latest_round, node_guard.id, dag_keys)
    }; // ✅ Drop locks immediately

    info!(
        "Node {}: DAG latest round: {}, Target round: {}",
        node_id, latest_round, current_round
    );

    // 🚀 **Optimization: Handle First Round**
    if current_round == 1 {
        info!(
            "Node {}: First round detected (round 1). Skipping DAG sync check.",
            node_id
        );
        return Ok(());
    }

    // ⚠️ **Check Synchronization Status**
    if latest_round < current_round - 1 {
        let error_message = format!(
            "Node {}: DAG not synchronized. Latest round in DAG: {}, required: {}.",
            node_id, latest_round, current_round - 1
        );
        error!("{}", error_message);
        return Err(error_message);
    }

    // ✅ **Synchronization Complete**
    info!(
        "Node {}: DAG is synchronized for round {} or beyond.",
        node_id, current_round - 1
    );

    Ok(())
}


pub fn check_size(shards: &[Vec<u8>], number_of_transactions: usize, transaction_size:usize) -> bool {
    let batch_size_limit = number_of_transactions * transaction_size;  // ✅ Dynamic limit

    let total_size: usize = shards.iter().map(|s| s.len()).sum();  // ✅ Sum up all shard sizes

    if total_size > batch_size_limit {
        info!(
            "Shard batch too large! Size: {} bytes (Max: {} bytes from config)",
            total_size, batch_size_limit
        );
        return false;
    }
    true
}

