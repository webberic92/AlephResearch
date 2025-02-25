use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::error;
use std::sync::Arc;

use crate::{
    handlers::{handle_commit::handle_commit, handle_prevote::handle_prevote, handle_propose::handle_propose}, structs::{
        node::Node,
        requests::{ CommitRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    }
};


/// **🔗 Initialize API Routes with `Arc<Mutex<Node>>`, `Client`, and `RBCProcessor`**
pub fn initialize_apis(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<Value>| {
                let node = node.clone();
                let client = client.clone();
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {
                            if let Err(e) = handle_propose(node, client, parsed_payload).await {
                                error!("Failed to handle proposal: {:?}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(Response { status: format!("Failed to handle proposal: {:?}", e) }),
                                );
                            }
                            (
                                StatusCode::OK,
                                Json(Response { status: "Proposal successfully handled.".to_string() }),
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
                            if let Err(e) = handle_prevote(node, client, parsed_payload).await {
                                error!("Failed to handle prevote: {:?}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(Response { status: format!("Failed to handle prevote: {:?}", e) }),
                                );
                            }
                            (
                                StatusCode::OK,
                                Json(Response { status: "Prevote successfully handled.".to_string() }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("Failed to parse PrevoteRequest: {:?}", err);
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
        .route("/commit", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<Value>| {
                let node = node.clone();
                let client = client.clone();
                async move {
                    match serde_json::from_value::<CommitRequest>(payload) {
                        Ok(parsed_payload) => {
                            if let Err(e) = handle_commit(node, client, parsed_payload).await {
                                error!("Failed to handle commit: {:?}", e);
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(Response { status: format!("Failed to handle commit: {:?}", e) }),
                                );
                            }
                            (
                                StatusCode::OK,
                                Json(Response { status: "Commit successfully handled.".to_string() }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("Failed to parse CommitRequest: {:?}", err);
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
}




