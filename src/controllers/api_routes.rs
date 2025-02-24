use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{error, info};
use std::sync::Arc;

use crate::{
    handlers::{ handle_commit::handle_commit, handle_dag_sync::handle_dag_sync, handle_prevote::handle_prevote, handle_propose::handle_propose, handle_sync_round::handle_sync_round},
    structs::{
        node::Node,
        requests::{ CommitRequest, DAGSyncRequest, PrevoteRequest, ProposeRequest, SyncroundRequest},
        responses::Response,
    }, utils::rbc_processor::RBCProcessor,
};

/// **🔗 Initialize API Routes with `Arc<Mutex<Node>>`.**
pub fn initialize_apis(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Router {
    let processor = Arc::new(RBCProcessor::new(node.clone(), client.clone())); // ✅ Fixed processor initialization

    Router::new()
        .route("/propose", post({
            let processor = processor.clone();
            move |Json(payload): Json<Value>| {
                let processor = processor.clone();
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {
                            processor.enqueue_proposal(parsed_payload).await; // ✅ Uses FIFO Queue
                            (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Proposal successfully enqueued.".to_string(),
                                }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("Failed to parse ProposeRequest: {:?}", err);
                            error!("{}", error_message);
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
        move |Json(payload): Json<Value>| {
            let node = node.clone();
            let client = client.clone();
            async move {
                match serde_json::from_value::<PrevoteRequest>(payload) {
                    Ok(parsed_payload) => {
                        match handle_prevote(node, client,parsed_payload).await {
                            Ok(_) => (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Prevote successfully handled.".to_string(),
                                }),
                            ),
                            Err(e) => {
                                let error_message = format!("Failed to handle prevote: {:?}", e);
                                tracing::error!("{}", error_message);
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(Response { status: error_message }),
                                )
                            }
                        }
                    }
                    Err(err) => {
                        let error_message = format!("Failed to parse PrevoteRequest: {:?}", err);
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
    .route("/commit", post({
        let node = node.clone();
        move |Json(payload): Json<Value>| {
            let node = node.clone();
            async move {
                match serde_json::from_value::<CommitRequest>(payload) {
                    Ok(parsed_payload) => {
                        match handle_commit(node, parsed_payload).await {
                            Ok(_) => (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Commit successfully handled.".to_string(),
                                }),
                            ),
                            Err(e) => {
                                let error_message = format!("Failed to handle commit: {:?}", e);
                                tracing::error!("{}", error_message);
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(Response { status: error_message }),
                                )
                            }
                        }
                    }
                    Err(err) => {
                        let error_message = format!("Failed to parse CommitRequest: {:?}", err);
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
    .route("/sync_epoch", post({
        let node = node.clone();
        move |Json(payload): Json<Value>| {
            let node = node.clone();
            async move {
                match serde_json::from_value::<SyncroundRequest>(payload) {
                    Ok(parsed_payload) => {
                        let sender_epoch = parsed_payload.round_id;
    
                        match handle_sync_round(node, sender_epoch).await {
                            Ok(_) => (
                                StatusCode::OK,
                                Json(Response {
                                    status: format!("Epoch successfully synced to {}", sender_epoch),
                                }),
                            ),
                            Err(err) => {
                                let error_message = format!("Failed to sync epoch: {:?}", err);
                                tracing::error!("{}", error_message);
                                (
                                    StatusCode::BAD_REQUEST,
                                    Json(Response { status: error_message }),
                                )
                            }
                        }
                    }
                    Err(err) => {
                        let error_message = format!("Failed to parse SyncEpochRequest: {:?}", err);
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
    .route("/health", axum::routing::get({
        let node = node.clone();
        move || {
            let node = node.clone();
            async move {
                let node_guard = node.lock().await;
                let quorum_votes = node_guard.quorum_votes.lock().await;
                let round_id = node_guard.current_round.lock().await;
    
                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, Rounds: {}",
                        node_guard.id, *quorum_votes, *round_id
                    ),
                })
            }
        }
    }))
    .route("/dag_sync", post({
        let node = node.clone();
        let client = client.clone();
        move |Json(payload): Json<DAGSyncRequest>| {
            let node = node.clone();
            let client = client.clone();
            async move {
                let node_guard = node.lock().await;
                info!(
                    "*** DAG SYNC REQUEST: Node {} {} from Sender {} ***",
                    node_guard.id, node_guard.ip_address, payload.sender_id
                );
                handle_dag_sync(node.clone(), client.clone(), Json(payload)).await
            }
        }
    }))
}
