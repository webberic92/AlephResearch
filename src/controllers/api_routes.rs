use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tracing::{error, info};
use std::sync::Arc;

use crate::{
    handlers::{handle_commit::handle_commit, handle_prevote::handle_prevote, handle_propose::handle_propose, handle_dag_sync::handle_dag_sync},
    structs::{
        node::Node,
        requests::{BaseRequest, DAGSyncRequest, PrevoteRequest, ProposeRequest, SyncEpochRequest},
        responses::Response,
    },
    utils::epoch_utils::handle_sync_epoch,
};

pub fn initialize_apis(node: Arc<Node>, client: Arc<Client>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<Value>| {
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {

                            // Handle the proposal and respond with proper HTTP status codes
                            handle_propose(
                                node.clone(),
                                client.clone(),
                                parsed_payload, // Pass the parsed ProposeRequest directly
                            )
                            .await
                        }
                        Err(err) => {
                            // Handle deserialization error
                            let error_message = format!("Failed to parse ProposeRequest: {:?}", err);
                            tracing::error!("{}", error_message);
                            (
                                StatusCode::BAD_REQUEST,
                                Json(Response { status: error_message }),
                            )
                        }
                    }
                }
            }
        }))  
        .route("/prevote", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<PrevoteRequest>| {
                let node = node.clone();
                let client = client.clone();
                async move {
                    handle_prevote(node.clone(),
                    client.clone(), payload).await
                }
            }
        }))
        // .route("/commit", post({
        //     let node = node.clone();
        //     let client = client.clone();
        //     move |Json(payload): Json<CommitRequest>| {
        //         async move {
        //             handle_commit(
        //                 &node,
        //                 client,
        //                 payload.sender,
        //                 payload.root,
        //                 payload.unit, // Added `unit` argument
        //                 payload.epoch_id, // Added `epoch_id` argument
        //                 payload.shard_hashes,
        //                 payload.proofs,
        //             )
        //             .await;
        //             Json(Response {
        //                 status: format!(
        //                     "Node {}: Commit accepted from Node {}",
        //                     node.id, payload.sender
        //                 ),
        //             })
        //         }
        //     }
        // }))
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
                    info!("*** outside handler DAG SYNC REQUEST: Node {} {} from Sender {} ***", node.id, node.ip_address, payload.sender_id);
                    handle_dag_sync(&node, &client, axum::Json(payload)).await
                }
            }
        }))
}
