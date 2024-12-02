use axum::{
    routing::{post},
    Json, Router,
};
use hyper::Server;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, Level};
use reed_solomon_erasure::galois_8::ReedSolomon;
use ring::digest::{Context, SHA256};
use tokio::time::Duration;
use toml;

// Constants for fault tolerance
const FAULT_TOLERANCE: usize = 1;
const MINIMUM_SHARES: usize = FAULT_TOLERANCE + 1;

// Data structures
#[derive(Clone)]
struct DagNode {
    round: usize,
    data: Vec<u8>,
    parents: Vec<usize>,
}

struct MerkleTree {
    root: Vec<u8>,
    branches: HashMap<usize, Vec<u8>>,
}

impl MerkleTree {
    fn new(data: &[Vec<u8>], n: usize) -> Self {
        let root = MerkleTree::hash_data(&data.concat());
        let branches = (0..n).map(|i| (i, data[i].clone())).collect();
        MerkleTree { root, branches }
    }

    fn branch(&self, index: usize) -> Option<Vec<u8>> {
        self.branches.get(&index).cloned()
    }

    fn hash_data(data: &[u8]) -> Vec<u8> {
        let mut context = Context::new(&SHA256);
        context.update(data);
        context.finish().as_ref().to_vec()
    }
}

// Configuration
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

enum Message {
    Propose { sender: usize, shard: Vec<u8>, proof: Vec<u8>, root: Vec<u8> },
    Prevote { sender: usize, root: Vec<u8> },
    Commit { sender: usize, root: Vec<u8> },
}

struct Node {
    id: usize,
    total_nodes: usize,
    fault_tolerance: usize,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    commit_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    data_storage: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
    config: Arc<Config>,
    message_tx: mpsc::Sender<Message>,
    message_rx: Mutex<mpsc::Receiver<Message>>,
}

impl Node {
    fn new(config: Arc<Config>) -> (Arc<Self>, mpsc::Sender<Message>) {
        let (tx, rx) = mpsc::channel(100);
        let node = Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            fault_tolerance: FAULT_TOLERANCE,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(HashMap::new())),
            data_storage: Arc::new(Mutex::new(HashMap::new())),
            config,
            message_tx: tx.clone(),
            message_rx: Mutex::new(rx),
        };
        (Arc::new(node), tx)
    }

    async fn handle_propose(&self, shard: Vec<u8>, proof: Vec<u8>, root: Vec<u8>) {
        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        if *counter >= (2 * self.fault_tolerance + 1) {
            for _peer in &self.config.network.nodes {
                let _ = self.message_tx.send(Message::Prevote {
                    sender: self.id,
                    root: root.clone(),
                }).await;
            }
        }
    }

    async fn handle_prevote(&self, root: Vec<u8>) {
        let mut commit_votes = self.commit_votes.write().await;
        let counter = commit_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        if *counter >= (self.fault_tolerance + 1) {
            for peer in &self.config.network.nodes {
                let _ = self.message_tx.send(Message::Commit {
                    sender: self.id,
                    root: root.clone(),
                }).await;
            }
        }
    }

    async fn handle_commit(&self, root: Vec<u8>) {
        let data = self.data_storage.lock().await.get(&root).cloned();
        if let Some(unit) = data {
            info!("Node {} outputs unit: {:?}", self.id, unit);
        }
    }

    fn check_size(&self, shard: &[u8]) -> bool {
        let transactions_in_shard = shard.len() / self.config.consensus.transaction_size;
        transactions_in_shard <= self.config.consensus.cb * self.config.consensus.batch_size
    }
}

// HTTP Handlers
async fn propose_handler(
    Json(payload): Json<ProposeRequest>,
    node: Arc<Node>,
) -> Json<Response> {
    node.handle_propose(payload.shard, payload.proof, payload.root).await;
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
    tracing_subscriber::fmt::init();

    let config = Arc::new(load_config("/home/aleph-node/aleph-node-config.toml"));
    let (node, _tx) = Node::new(config.clone());

    let app = Router::new()
        .route("/propose", post(|payload| propose_handler(payload, node.clone())))
        .route("/prevote", post(|payload| prevote_handler(payload, node.clone())))
        .route("/commit", post(|payload| commit_handler(payload, node.clone())));

    let addr = config.network.listen_address.parse::<SocketAddr>().unwrap();
    Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
}
