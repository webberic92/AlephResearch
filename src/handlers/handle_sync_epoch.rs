use tracing::{error, info};

use crate::{structs::node::Node, utils::config_util::persist_epoch_round_id};

use axum::{Json, http::StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::structs::{ requests::SyncEpochRequest, responses::Response};

/// Handles an incoming sync epoch request.
pub async fn handle_sync_epoch(
    node: Arc<RwLock<Node>>,
    Json(payload): Json<SyncEpochRequest>,
) -> (StatusCode, Json<Response>) {
    let node_id = {
        let node_read = node.read().await;
        node_read.id
    };

    info!(
        "Node {}: ==== Handling SYNC EPOCH request from Node {} for Epoch {} ====",
        node_id, payload.sender, payload.epoch_id
    );

    {
        let node_write = node.write().await;
        let mut tracker = node_write.epoch_round_id.lock().await;

        if tracker.contains(&payload.epoch_id) {
            info!(
                "Node {}: Epoch {} already synchronized from Node {}",
                node_id, payload.epoch_id, payload.sender
            );
            return (
                StatusCode::OK,
                Json(Response {
                    status: format!(
                        "Node {}: Epoch {} already synchronized from {}",
                        node_id, payload.epoch_id, payload.sender
                    ),
                }),
            );
        }

        // Update the tracker
        tracker.insert(payload.epoch_id);
    }

    info!(
        "Node {}: Epoch {} synchronized successfully from Node {}",
        node_id, payload.epoch_id, payload.sender
    );

    // Persist the updated epoch to configuration
    if let Err(e) = persist_epoch_round_id(payload.epoch_id).await {
        let error_message = format!(
            "Node {}: Failed to persist Epoch {} to TOML: {:?}",
            node_id, payload.epoch_id, e
        );
        error!("{}", error_message);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Response { status: error_message }),
        );
    }

    (
        StatusCode::OK,
        Json(Response {
            status: format!(
                "Node {}: Epoch {} synchronized successfully from {}",
                node_id, payload.epoch_id, payload.sender
            ),
        }),
    )
}
