use axum::{routing::post, Json, Router};
use reqwest::Client;
use std::sync::Arc;

use crate::{
    handlers::{handle_commit::handle_commit, handle_prevote::handle_prevote, handle_propose::handle_propose, handle_dag_sync::handle_dag_sync},
    structs::{
        node::Node,
        requests::{CommitRequest, DAGSyncRequest, PrevoteRequest, ProposeRequest, SyncEpochRequest},
        responses::Response,
    },
    utils::epoch_utils::handle_sync_epoch,
};

pub fn initialize_apis(node: Arc<Node>, client: Arc<Client>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<ProposeRequest>| {
                async move {
                    handle_propose(
                        &node,
                        &client,
                        payload.sender,
                        payload.root,
                        payload.proof,
                        &payload.shard,
                        payload.epoch_id,
                    )
                    .await;
                    Json(Response {
                        status: format!(
                            "Node {}: Propose accepted from Node {} for epoch {}",
                            node.id, payload.sender, payload.epoch_id
                        ),
                    })
                }
            }
        }))
        .route("/prevote", post({
            let node = node.clone();
            let client = client.clone(); // Clone the `client` variable
            move |Json(payload): Json<PrevoteRequest>| {
                async move {
                    handle_prevote(
                        &node,
                        &client, // Use the cloned `client` variable
                        payload.sender,
                        payload.root,
                        payload.proof,
                        payload.shard,
                        payload.epoch_id, // Added `epoch_id` argument
                        &payload.node_url,
                        // Updated to use `unit` instead of `shard`
                    )
                    .await;
                    Json(Response {
                        status: format!(
                            "Node {}: Prevote accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
        .route("/commit", post({
            let node = node.clone();
            move |Json(payload): Json<CommitRequest>| {
                async move {
                    handle_commit(
                        &node,
                        payload.sender,
                        payload.root,
                        payload.unit, // Added `unit` argument
                        payload.epoch_id, // Added `epoch_id` argument
                    )
                    .await;
                    Json(Response {
                        status: format!(
                            "Node {}: Commit accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
        .route("/sync_epoch", post({
            let node = node.clone();
            move |Json(payload): Json<SyncEpochRequest>| {
                async move {
                    match handle_sync_epoch(&node, payload.epoch_id, payload.sender).await {
                        Ok(_) => Json(Response {
                            status: format!(
                                "Node {}: Epoch {} synchronized successfully from {}",
                                node.id, payload.epoch_id, payload.sender
                            ),
                        }),
                        Err(e) => Json(Response {
                            status: format!(
                                "Node {}: Failed to synchronize epoch {} from {}: {}",
                                node.id, payload.epoch_id, payload.sender, e
                            ),
                        }),
                    }
                }
            }
        }))
        .route("/health", axum::routing::get({
            let node = node.clone();
            move || async move {
                let quorum_votes = node.quorum_votes.read().await;
                let epoch_round_id = node.epoch_round_id.lock().await;

                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, Epochs: {:?}",
                        node.id, *quorum_votes, *epoch_round_id
                    ),
                })
            }
        }))
        .route("/dag_sync", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<DAGSyncRequest>| {
                let node = node.clone();
                let client = client.clone();
                async move {
                    handle_dag_sync(&node, &client, axum::Json(payload)).await
                }
            }
        }))
}
