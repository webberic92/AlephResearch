use axum::{extract::DefaultBodyLimit, routing::post, Json, Router};
use reqwest::StatusCode;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::error;
use std::sync::{atomic::Ordering, Arc};

use crate::{
    processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor},
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    },
};


/// **🔗 Initialize API Routes with `Arc<Mutex<Node>>`, `Client`, and `RBCProcessor`**
pub fn initialize_apis(node: Arc<Mutex<Node>>,rbc_processor: Arc<RBCProcessor>) -> Router {
    Router::new()
        .route("/propose", post({
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {
                            let propose_message = RBCMessage::Proposal(parsed_payload);
                            rbc_processor.enqueue_message(propose_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "✅ Proposal successfully enqueued.".to_string() }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("❌ Failed to parse ProposeRequest: {:?}", err);
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
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<PrevoteRequest>(payload) {
                        Ok(parsed_payload) => {

                            let prevote_message = RBCMessage::Prevote(parsed_payload);
                            rbc_processor.enqueue_message(prevote_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "✅ Prevote successfully enqueued.".to_string() }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("❌ Failed to parse PrevoteRequest: {:?}", err);
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
            let rbc_processor = rbc_processor.clone();
            move |Json(payload): Json<Value>| {
                let rbc_processor = rbc_processor.clone();
                async move {
                    match serde_json::from_value::<CommitRequest>(payload) {
                        Ok(parsed_payload) => {

                            let commit_message = RBCMessage::Commit(parsed_payload);
                            rbc_processor.enqueue_message(commit_message).await;
                            (
                                StatusCode::OK,
                                Json(Response { status: "✅ Commit successfully enqueued.".to_string() }),
                            )
                        }
                        Err(err) => {
                            let error_message = format!("❌ Failed to parse CommitRequest: {:?}", err);
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
                    let node_guard = node.lock().await; // ✅ Lock node once
                    let quorum_votes = node_guard.quorum_votes.lock().await;
                    let round_id = node_guard.current_round.lock().await;
        
                    // ✅ Directly increment the message count without locking the whole node
                    node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        
                    Json(Response {
                        status: format!(
                            "Node {} is healthy. Quorum votes: {:?}, Rounds: {}",
                            node_guard.id, *quorum_votes, *round_id
                        ),
                    })
                }
            }
        }))
        .layer(DefaultBodyLimit::disable())
}
