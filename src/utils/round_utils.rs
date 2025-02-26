use std::sync::Arc;

use reqwest::Client;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};
use tokio::time::timeout;
use std::time::Duration;
use crate::structs::{node::Node, requests::SyncroundRequest};


// pub async fn update_local_round(node: Arc<Mutex<Node>>) -> Result<(), Box<dyn std::error::Error>> {
//     let mut node_guard = node.lock().await; // 🔥 Lock entire node
//     let node_id = node_guard.id;
//     node_guard.current_round += 1;

//     info!(
//         "Node {}: Updated local round to the next round: {}",
//         node_id, node_guard.current_round
//     );

//     Ok(())
// }


/// Updates the round in the local node state.
// 



/// **🔄 Correctly Update Local Round**  
/// - **Locks the `Mutex`** for atomic round increment.  
/// - **Avoids arithmetic on `Arc`**, which isn't allowed.
pub async fn update_local_round(node: Arc<Mutex<Node>>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Attempting to acquire lock on node...");

    info!("🔍 [DEBUG] Waiting to acquire node lock for rount utils");
let node_guard = node.lock().await;
info!("🔓 [DEBUG] Acquired node lock for rount utils"); // 🔒 Lock Node
    let node_id = node_guard.id;

    info!("Lock acquired on node {}. Attempting to update current round...", node_id);

    {
        let mut current_round_guard = node_guard.current_round.lock().await; // 🔒 Lock current_round
        *current_round_guard += 1;

        info!(
            "Node {}: Updated local round to the next round: {}",
            node_id, *current_round_guard
        );
    } // 🔓 Drop current_round lock immediately

    info!("Node {}: Completed update_local_round.", node_id);

    Ok(())
}





/// Broadcasts the updated round to all nodes.
pub async fn broadcast_round_update(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    round_id: u64,
) -> Result<(), String> {
    info!("Node {}: Entering broadcast_round_update for round {}", node.read().await.id, round_id);

    let node_urls = {
        let node_state = node.read().await;
        node_state.nodes.clone()
    };

    info!(
        "Node {}: About to send sync_round requests to all nodes: {:?}",
        node.read().await.id, node_urls
    );

    let mut failed_syncs = Vec::new(); // Track failures

    for node_url in node_urls {
        let sync_url = format!("http://{}/sync_round", node_url);

        let sync_round_request = SyncroundRequest {
            round_id,
            sender: node.read().await.id,
        };
        info!(
            "Node {}: Broadcasting round update for round {} to node {}",
            node.read().await.id, round_id, node_url
        );
        match timeout(Duration::from_secs(5), client.post(&sync_url).json(&sync_round_request).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                info!(
                    "Node {}: Successfully synced round {} with node {}",
                    node.read().await.id, round_id, node_url
                );
            }
            Ok(Ok(response)) => {
                let error_message = format!(
                    "Node {}: Failed to sync round {} with node {}. HTTP {}",
                    node.read().await.id, round_id, node_url, response.status()
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Ok(Err(e)) => {
                let error_message = format!(
                    "Node {}: Error syncing round {} with node {}: {:?}",
                    node.read().await.id, round_id, node_url, e
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
            Err(_) => {
                let error_message = format!(
                    "Node {}: Timeout while syncing round {} with node {}",
                    node.read().await.id, round_id, node_url
                );
                error!("{}", error_message);
                failed_syncs.push(error_message);
            }
        }
    }

    if !failed_syncs.is_empty() {
        error!(
            "Node {}: Some round syncs failed: {:?}",
            node.read().await.id, failed_syncs
        );
        // Don't return an error to avoid blocking progress
    }

    info!("Node {}: Finished broadcasting round update for round {}", node.read().await.id, round_id);

    Ok(())
}


