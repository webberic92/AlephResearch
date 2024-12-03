use axum::{
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, Level};

// Constants for fault tolerance
const FAULT_TOLERANCE: usize = 1;

// Configuration structures
#[derive(Debug, Deserialize)]
struct Config {
    network: NetworkConfig,
    consensus: ConsensusConfig,
    logging: LoggingConfig,
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
    round: usize,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    transaction_metrics_log: String,
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    id: usize,
    total_nodes: usize,
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

#[derive(Debug, Clone)] // Add Clone here
struct Node {
    id: usize,
    total_nodes: usize,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    commit_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    config: Arc<Config>,
    transaction_count: usize,
}

impl Node {
    fn new(config: Arc<Config>) -> Self {
        Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(HashMap::new())),
            config,
            transaction_count: 0,
        }
    }

    async fn process_transactions(&mut self) {
        info!(
            "Node {} starting transactions. Batch size: {}, Transaction size: {} bytes",
            self.id, self.config.consensus.batch_size, self.config.consensus.transaction_size
        );

        for i in 0..self.config.consensus.batch_size {
            self.transaction_count += 1;
            info!(
                "Node {} processed transaction {}/{}",
                self.id, i + 1, self.config.consensus.batch_size
            );
        }

        info!("Node {} completed all transactions.", self.id);
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

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// HTTP Handlers
async fn propose_handler(Json(payload): Json<ProposeRequest>, node: Arc<RwLock<Node>>) -> Json<Response> {
    let node = node.write().await; // Acquire write lock for mutation
    node.handle_propose(payload.root).await;
    Json(Response {
        status: "Propose accepted".to_string(),
    })
}

async fn prevote_handler(Json(payload): Json<PrevoteRequest>, node: Arc<RwLock<Node>>) -> Json<Response> {
    let node = node.write().await; // Acquire write lock for mutation
    node.handle_prevote(payload.root).await;
    Json(Response {
        status: "Prevote accepted".to_string(),
    })
}

async fn commit_handler(Json(payload): Json<CommitRequest>, node: Arc<RwLock<Node>>) -> Json<Response> {
    let node = node.write().await; // Acquire write lock for mutation
    node.handle_commit(payload.root).await;
    Json(Response {
        status: "Commit accepted".to_string(),
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let config = Arc::new(load_config("/home/aleph-node/aleph-node-config.toml"));

    info!(
        "Node {} starting. Listening on {}. Total nodes: {}",
        config.node.id,
        config.network.listen_address,
        config.node.total_nodes
    );

    let node = Arc::new(RwLock::new(Node::new(config.clone())));

    // Start transaction processing
    let node_clone = node.clone();
    tokio::spawn(async move {
        let mut node = node_clone.write().await; // Acquire write lock
        node.process_transactions().await;
    });

    let app = Router::new()
    .route("/propose", {
        let node = node.clone();
        post(move |payload| propose_handler(payload, node.clone()))
    })
    .route("/prevote", {
        let node = node.clone();
        post(move |payload| prevote_handler(payload, node.clone()))
    })
    .route("/commit", {
        let node = node.clone();
        post(move |payload| commit_handler(payload, node.clone()))
    });

    let addr: SocketAddr = config.network.listen_address.parse().expect("Invalid listen address");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    info!("Server running on {}", addr);

    axum::serve(listener, app)
        .await
        .expect("Server failed to start");
}