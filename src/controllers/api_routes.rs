use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;
use std::sync::Arc;

use crate::{
    handlers::{ handle_commit::handle_commit, handle_dag_sync::handle_dag_sync, handle_prevote::handle_prevote, handle_propose::handle_propose, handle_sync_epoch::handle_sync_epoch},
    structs::{
        node::Node,
        requests::{ CommitRequest, DAGSyncRequest, PrevoteRequest, ProposeRequest, SyncEpochRequest},
        responses::Response,
    },
};

pub fn initialize_apis(node: Arc<RwLock<Node>>, client: Arc<Client>) -> Router {
    Router::new()
    
    .route("/propose", post({
        let node = node.clone(); // Clone the Arc to move it into the closure
        let client = client.clone(); // Clone the Arc to move it into the closure
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            let client = client.clone(); // Clone Arc for each request
            async move {
                match serde_json::from_value::<ProposeRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the proposal and respond with proper HTTP status codes
                        handle_propose(
                            node,               // Pass the cloned Arc<RwLock<Node>>
                            client,             // Pass the cloned Arc<Client>
                            parsed_payload,     // Pass the parsed ProposeRequest directly
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
        let node = node.clone(); // Clone the Arc to move it into the closure
        let client = client.clone(); // Clone the Arc to move it into the closure
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            let client = client.clone(); // Clone Arc for each request
            async move {
                match serde_json::from_value::<PrevoteRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the prevote and respond with proper HTTP status codes
                        handle_prevote(
                            node,               // Pass the cloned Arc<RwLock<Node>>
                            client,             // Pass the cloned Arc<Client>
                            parsed_payload,     // Pass the parsed PrevoteRequest directly
                        )
                        .await
                    }
                    Err(err) => {
                        // Handle deserialization error
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
        let node = node.clone(); // Clone the Arc for the node
        let client = client.clone(); // Clone the Arc for the client
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            let client = client.clone(); // Clone Arc for each request
            async move {
                // Parse the JSON payload into the `CommitRequest` struct
                match serde_json::from_value::<CommitRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the commit and respond with proper HTTP status codes
                        match handle_commit(node, client, parsed_payload).await {
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
                        // Handle deserialization error
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
        let node = node.clone(); // Clone the Arc for the node
        move |Json(payload): Json<SyncEpochRequest>| {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                match handle_sync_epoch(node, Json(payload)).await {
                    (status, response) => (status, response),
                }
            }
        }
    }))
    
    .route("/health", axum::routing::get({
        let node = node.clone(); // Clone the Arc for the node
        move || {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                let node_read = node.read().await;
                let quorum_votes = node_read.quorum_votes.read().await;
                let epoch_round_id = node_read.current_epoch.lock().await;
    
                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, Epochs: {:?}",
                        node_read.id, *quorum_votes, *epoch_round_id
                    ),
                })
            }
        }
    }))
    
    .route("/dag_sync", post({
        let node = node.clone(); // Clone the Arc for the node
        let client = client.clone(); // Clone the Arc for the client
        move |Json(payload): Json<DAGSyncRequest>| {
            let node = node.clone(); // Clone Arc again for each request
            let client = client.clone(); // Clone Arc for each request
            async move {
                let node_read = node.read().await;
                info!(
                    "*** DAG SYNC REQUEST: Node {} {} from Sender {} ***",
                    node_read.id, node_read.ip_address, payload.sender_id
                );
    
                // Call `handle_dag_sync` with proper Arc handling
                handle_dag_sync(node.clone(), client.clone(), Json(payload)).await
            }
        }
    }))
}
