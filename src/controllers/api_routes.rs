use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{error, info};
use std::sync::{atomic::Ordering, Arc};

use crate::{
    processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor},
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    },
};

/// ✅ **Helper Method to Validate Incoming Requests**
async fn should_process_request(node: Arc<Mutex<Node>>, request_round: u64) -> bool {
    let node_guard = node.lock().await;
    let dag_guard = node_guard.dag.lock().await;

    if dag_guard.contains_key(&request_round) {
        info!(
            "🛑  Node {}: Round {} is already finalized in DAG. Denying request.",
            node_guard.id, request_round
        );
        return false;
    }

    true
}


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
                            if !should_process_request(node.clone(), parsed_payload.base.round_id).await {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    Json(Response { status: "🛑 Request dropped: Node has reached final round.".to_string() }),
                                );
                            }

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
                            if !should_process_request(node.clone(),parsed_payload.proposals[0].base.round_id).await {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    Json(Response { status: "🛑 Request dropped: Node has reached final round.".to_string() }),
                                );
                            }

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
                            if !should_process_request(node.clone(), parsed_payload.round_id).await {
                                return (
                                    StatusCode::BAD_REQUEST,
                                    Json(Response { status: "🛑 Request dropped: Node has reached final round.".to_string() }),
                                );
                            }

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
}
