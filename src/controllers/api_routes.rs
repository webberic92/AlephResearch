use axum::{routing::post, Json, Router};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::error;
use std::sync::Arc;

use crate::{
    processors::{process_commits::RBCProcessorCommit, process_prevotes::RBCProcessorPrevote, process_proposals::RBCProcessorProposal}, structs::{
        node::Node,
        requests::{ CommitRequest, PrevoteRequest, ProposeRequest},
        responses::Response,
    },
};

/// **🔗 Initialize API Routes with `Arc<Mutex<Node>>`.**
pub fn initialize_apis(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Router {
    let processor_proposal = Arc::new(RBCProcessorProposal::new(node.clone(), client.clone())); // ✅ Fixed processor initialization
    let processor_prevote = Arc::new(RBCProcessorPrevote::new(node.clone(), client.clone())); // ✅ Processor for prevotes
    let processor_commit = Arc::new(RBCProcessorCommit::new(node.clone(), client.clone())); // ✅ Processor for commits
    Router::new()
        .route("/propose", post({
            let processor_proposal = processor_proposal.clone();
            move |Json(payload): Json<Value>| {
                let processor_proposal = processor_proposal.clone();
                async move {
                    match serde_json::from_value::<ProposeRequest>(payload) {
                        Ok(parsed_payload) => {
                            processor_proposal.enqueue_proposal(parsed_payload).await; // ✅ Uses FIFO Queue
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
            let processor = processor_prevote.clone();
            move |Json(payload): Json<Value>| {
                let processor = processor.clone();
                async move {
                    match serde_json::from_value::<PrevoteRequest>(payload) {
                        Ok(parsed_payload) => {
                            processor.enqueue_prevote(parsed_payload).await; // ✅ Uses FIFO Queue
                            (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Prevote successfully enqueued.".to_string(),
                                }),
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
            let processor = processor_commit.clone();
            move |Json(payload): Json<Value>| {
                let processor = processor.clone();
                async move {
                    match serde_json::from_value::<CommitRequest>(payload) {
                        Ok(parsed_payload) => {
                            processor.enqueue_commit(parsed_payload).await; // ✅ Uses FIFO Queue
                            (
                                StatusCode::OK,
                                Json(Response {
                                    status: "Commit successfully enqueued.".to_string(),
                                }),
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
