use axum::{
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock};
use tracing::{info, Level};
use toml;

// Constants for fault tolerance
const FAULT_TOLERANCE: usize = 1;
const MINIMUM_SHARES: usize = FAULT_TOLERANCE + 1;

// Configuration structures
#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    consensus: ConsensusConfig,
    node: NodeConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    listen_address: String,
    nodes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConsensusConfig {
    batch_size: usize,
    transaction_size: usize,
    cb: usize,
    round: usize,
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    id: usize,
    total_nodes: usize,
}

fn load_config(file_path: &str) -> Config {
    let config_contents = std::fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// Message types
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

struct Node {
    id: usize,
    total_nodes: usize,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    commit_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    config: Arc<Config>,
}

impl Node {
    fn new(config: Arc<Config>) -> Arc<Self> {
        Arc::new(Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    async fn handle_propose(&self, root: Vec<u8>) {
        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;
        info!("Node {} received propose. Quorum: {}", self.id, counter);
    }

    async fn handle_prevote(&self, root: Vec<u8>) {
        let mut commit_votes = self.commit_votes.write().await;
        let counter = commit_votes.entry(root.clone()).or_insert(0);
        *counter += 1;
        info!("Node {} received prevote. Commit count: {}", self.id, counter);
    }

    async fn handle_commit(&self, root: Vec<u8>) {
        info!("Node {} received commit for root {:?}", self.id, root);
    }
}

// HTTP Handlers
async fn propose_handler(Json(payload): Json<ProposeRequest>, node: Arc<Node>) -> Json<Response> {
    node.handle_propose(payload.root).await;
    Json(Response {
        status: "Propose accepted".to_string(),
    })
}

async fn prevote_handler(Json(payload): Json<PrevoteRequest>, node: Arc<Node>) -> Json<Response> {
    node.handle_prevote(payload.root).await;
    Json(Response {
        status: "Prevote accepted".to_string(),
    })
}

async fn commit_handler(Json(payload): Json<CommitRequest>, node: Arc<Node>) -> Json<Response> {
    node.handle_commit(payload.root).await;
    Json(Response {
        status: "Commit accepted".to_string(),
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let config = Arc::new(load_config("/home/aleph-node/aleph-node-config.toml"));
    let node = Node::new(config.clone());

    let app = Router::new()
        .route("/propose", post(|payload| propose_handler(payload, node.clone())))
        .route("/prevote", post(|payload| prevote_handler(payload, node.clone())))
        .route("/commit", post(|payload| commit_handler(payload, node.clone())));

    let addr = config.network.listen_address.parse::<SocketAddr>().unwrap();

    // Use Axum's built-in `serve`
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("Server failed to start");
}
