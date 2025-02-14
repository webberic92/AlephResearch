use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;
use std::sync::Arc;

use crate::{
    handlers::{ handle_commit::handle_commit, handle_dag_sync::handle_dag_sync, handle_prevote::handle_prevote, handle_propose::handle_propose, handle_sync_round::handle_sync_round},
    structs::{
        node::Node,
        requests::{ CommitRequest, DAGSyncRequest, PrevoteRequest, ProposeRequest, SyncroundRequest},
        responses::Response,
    },
};

pub fn initialize_apis(node: Arc<RwLock<Node>>, client: Arc<Client>) -> Router {
    Router::new()
    .route("/propose", post({
        let node = node.clone(); // Clone the Arc to move it into the closure
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                match serde_json::from_value::<ProposeRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the proposal and respond with proper HTTP status codes
                        match handle_propose(node, parsed_payload).await {
                            Ok(_) => (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Proposal successfully handled.".to_string(),
                                }),
                            ),
                            Err(e) => {
                                let error_message = format!("Failed to handle proposal: {:?}", e);
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
        let node = node.clone(); // Clone the Arc for the node
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                // Parse the JSON payload into the `PrevoteRequest` struct
                match serde_json::from_value::<PrevoteRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the prevote and respond with proper HTTP status codes
                        match handle_prevote(node, parsed_payload).await {
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
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                // Parse the JSON payload into the `CommitRequest` struct
                match serde_json::from_value::<CommitRequest>(payload) {
                    Ok(parsed_payload) => {
                        // Handle the commit and respond with proper HTTP status codes
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
    .route("/sync_round", post({
        let node = node.clone(); // Clone the Arc for the node
        move |Json(payload): Json<Value>| {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                match serde_json::from_value::<SyncroundRequest>(payload) {
                    Ok(parsed_payload) => {
                        let sender_round = parsed_payload.round_id; // Extract round ID from request
    
                        match handle_sync_round(node, sender_round).await {
                            Ok(_) => (
                                StatusCode::OK,
                                Json(Response {
                                    status: format!("round successfully synced to {}", sender_round),
                                }),
                            ),
                            Err(err) => {
                                let error_message = format!("Failed to sync round: {:?}", err);
                                tracing::error!("{}", error_message);
                                (
                                    StatusCode::BAD_REQUEST,
                                    Json(Response { status: error_message }),
                                )
                            }
                        }
                    }
                    Err(err) => {
                        let error_message = format!("Failed to parse SyncroundRequest: {:?}", err);
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
        let node = node.clone(); // Clone the Arc for the node
        move || {
            let node = node.clone(); // Clone Arc again for each request
            async move {
                let node_read = node.read().await;
                let quorum_votes = node_read.quorum_votes.read().await;
                let round_id = node_read.current_round.lock().await;
    
                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, rounds: {:?}",
                        node_read.id, *quorum_votes, *round_id
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
