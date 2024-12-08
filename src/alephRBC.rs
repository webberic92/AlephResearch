use axum::{routing::post, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tracing::{info, error};
use serde::{Deserialize, Serialize};
use tracing_subscriber;

#[derive(Deserialize)]
struct ProposeRequest {
    sender: usize,
    shard: Vec<u8>,
    proof: Vec<u8>,
    root: Vec<u8>,
}

#[derive(Deserialize)]
struct PrevoteRequest {
    sender: usize,
    root: Vec<u8>,
}

#[derive(Deserialize)]
struct CommitRequest {
    sender: usize,
    root: Vec<u8>,
}

#[derive(Serialize)]
struct Response {
    status: String,
}

#[derive(Debug, Clone)]
struct Node {
    id: usize,
    quorum_votes: Arc<RwLock<usize>>,
    commit_votes: Arc<RwLock<usize>>,
}

impl Node {
    fn new(id: usize) -> Self {
        Self {
            id,
            quorum_votes: Arc::new(RwLock::new(0)),
            commit_votes: Arc::new(RwLock::new(0)),
        }
    }

    // Phase 1: Proposal Phase
    async fn handle_propose(&self) {
        info!("Node {} handling propose request", self.id);
        let mut quorum_votes = self.quorum_votes.write().await;
        *quorum_votes += 1;
    }

    // Phase 2: Prevote Phase
    async fn handle_prevote(&self) {
        info!("Node {} handling prevote request", self.id);
        let mut commit_votes = self.commit_votes.write().await;
        *commit_votes += 1;
    }

    // Phase 3: Commit Phase
    async fn handle_commit(&self) {
        info!("Node {} handling commit request", self.id);
    }

    // Phase 4: Output Phase TBD
    async fn handle_output(&self, root: Vec<u8>) {
        info!("This is the output phase. Node {} received output for root {:?}", self.id, root);
    }
}

fn initialize_apis(node: Arc<Node>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                async move {
                    node.handle_propose().await;
                    Json(Response {
                        status: "Propose accepted".to_string(),
                    })
                }
            }
        }))
        .route("/prevote", post({
            let node = node.clone();
            move |Json(payload): Json<PrevoteRequest>| {
                let node = node.clone();
                async move {
                    node.handle_prevote().await;
                    Json(Response {
                        status: "Prevote accepted".to_string(),
                    })
                }
            }
        }))
        .route("/commit", post({
            let node = node.clone();
            move |Json(payload): Json<CommitRequest>| {
                let node = node.clone();
                async move {
                    node.handle_commit().await;
                    Json(Response {
                        status: "Commit accepted".to_string(),
                    })
                }
            }
        }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let node = Arc::new(Node::new(1));
    let app = initialize_apis(node);

    let addr = "0.0.0.0:30333".parse::<std::net::SocketAddr>()?;
    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
