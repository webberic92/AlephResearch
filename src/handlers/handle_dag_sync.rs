use std::sync::Arc;

use axum::Json;
use reqwest::Client;
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{error, info};
use crate::structs::requests::DAGSyncRequest;
use crate::utils::dag_utils::check_dag_sync;
use crate::structs::node::Node;

#[derive(Serialize)]
pub struct DAGSyncResponse {
    pub in_sync: bool,
    pub message: String,
}


/// Handles the DAG synchronization API request.
///
/// Checks whether the local DAG is in sync with the requesting node.
pub async fn handle_dag_sync(
    node: Arc<RwLock<Node>>, // Now takes an Arc<RwLock<Node>> for thread safety
    client: Arc<Client>, // Ensure the client is cloned properly
    Json(payload): Json<DAGSyncRequest>, // Deserialize the payload automatically
) -> Json<DAGSyncResponse> {
    // Acquire a read lock to access node properties safely
    let node_read = node.read().await;
    
    info!(
        "Node {}: INSIDE Handler - Received DAG sync request from Node {} for round {}. About to call check_dag_sync...",
        node_read.id, payload.sender_id, payload.round_id
    );

    // Call `check_dag_sync` asynchronously
    match check_dag_sync(&client, payload.round_id, &payload.sender_id, &payload.sender_url).await {
        Ok(in_sync) => {
            if in_sync {
                Json(DAGSyncResponse {
                    in_sync: true,
                    message: format!(
                        "Node {}: DAG is in sync with Node {} for round {}",
                        node_read.id, payload.sender_id, payload.round_id
                    ),
                })
            } else {
                Json(DAGSyncResponse {
                    in_sync: false,
                    message: format!(
                        "Node {}: DAG is NOT in sync with Node {} for round {}",
                        node_read.id, payload.sender_id, payload.round_id
                    ),
                })
            }
        }
        Err(e) => {
            error!(
                "Node {}: Failed to check DAG sync with Node {} for round {}. Error: {:?}",
                node_read.id, payload.sender_id, payload.round_id, e
            );
            Json(DAGSyncResponse {
                in_sync: false,
                message: format!("Error during DAG sync check: {:?}", e),
            })
        }
    }
}


