use axum::Json;
use serde::Serialize;
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
    node: &Node,
    client: &reqwest::Client,
    Json(payload): Json<DAGSyncRequest>, // Deserialize the payload automatically
) -> Json<DAGSyncResponse> {
    info!(
        "Node {}: Received DAG sync request from Node {} for epoch {}",
        node.id, payload.senderId, payload.epoch_id
    );

    match check_dag_sync(client, payload.epoch_id, &payload.senderId, &payload.senderUrl).await {
        Ok(in_sync) => {
            if in_sync {
                Json(DAGSyncResponse {
                    in_sync: true,
                    message: format!(
                        "Node {}: DAG is in sync with Node {} for epoch {}",
                        node.id, payload.senderId, payload.epoch_id
                    ),
                })
            } else {
                Json(DAGSyncResponse {
                    in_sync: false,
                    message: format!(
                        "Node {}: DAG is NOT in sync with Node {} for epoch {}",
                        node.id, payload.senderId, payload.epoch_id
                    ),
                })
            }
        }
        Err(e) => {
            error!(
                "Node {}: Failed to check DAG sync with Node {} for epoch {}. Error: {:?}",
                node.id, payload.senderId, payload.epoch_id, e
            );
            Json(DAGSyncResponse {
                in_sync: false,
                message: format!("Error during DAG sync check: {:?}", e),
            })
        }
    }
}

