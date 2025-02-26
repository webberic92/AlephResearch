use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{error, info};
use std::sync::Arc;

use crate::{
    processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor},
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    },
};

/// **🔗 Initialize API Routes with `Arc<Mutex<Node>>`, `Client`, and `RBCProcessor`**
pub fn initialize_apis(node: Arc<Mutex<Node>>, client: Arc<Client>, rbc_processor: Arc<RBCProcessor>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let node = node.clone();
                let client = client.clone();
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {
                            let propose_message = RBCMessage::Proposal(parsed_payload);
                            rbc_processor.enqueue_message(propose_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "Proposal successfully enqueued.".to_string() }),
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
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let node = node.clone();
                let client = client.clone();
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<PrevoteRequest>(payload) {
                        Ok(parsed_payload) => {
                            let prevote_message = RBCMessage::Prevote(parsed_payload);
                            rbc_processor.enqueue_message(prevote_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "Prevote successfully enqueued.".to_string() }),
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
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let node = node.clone();
                let client = client.clone();
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<CommitRequest>(payload) {
                        Ok(parsed_payload) => {
                            let commit_message = RBCMessage::Commit(parsed_payload);
                            rbc_processor.enqueue_message(commit_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "Commit successfully enqueued.".to_string() }),
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
                    info!("🔍 [DEBUG] Waiting to acquire node lock for health API");
                    let node_guard = node.lock().await;
                    info!("🔓 [DEBUG] Acquired node lock for health API");
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
