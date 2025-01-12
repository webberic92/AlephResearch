use tracing::{error, info};
use crate::structs::node::Node;
use crate::utils::dag_utils::check_dag_sync;
use reqwest::Client;

/// Attempts to recover from a reconstruction failure by ensuring DAG synchronization and revalidating shards.
pub async fn attempt_recovery(
    node: &Node,
    client: &Client,
    epoch_id: u64,
    node_url: &str,
) -> Result<(), String> {
    info!("Node {}: Starting recovery process for epoch {}", node.id, epoch_id);

    // Check DAG synchronization
    match check_dag_sync(client, epoch_id, node_url).await {
        Ok(true) => {
            info!("Node {}: DAG synchronization successful with {}", node.id, node_url);
            Ok(())
        }
        Ok(false) => {
            error!("Node {}: DAG not in sync with {}", node.id, node_url);
            Err(format!("DAG not in sync with node {}", node_url))
        }
        Err(e) => {
            error!(
                "Node {}: Failed to check DAG sync with node {}. Error: {:?}",
                node.id, node_url, e
            );
            Err(format!("Failed DAG sync check: {:?}", e))
        }
    }
}
