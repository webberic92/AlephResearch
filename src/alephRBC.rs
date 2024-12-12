use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::SocketAddr,
    sync::Arc,
};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};
use tracing_subscriber;

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
    data_shards: usize,
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
    proof: Vec<Vec<u8>>,
    root: Vec<u8>,
    epoch_id: u64,
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

// Node structure
#[derive(Debug, Clone)]
struct Node {
    id: usize,
    total_nodes: usize,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    epoch_tracker: Arc<Mutex<HashSet<u64>>>,
}

impl Node {
    fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            epoch_tracker: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn f(&self) -> usize {
        (self.total_nodes - 1) / 3 // Fault tolerance based on total nodes
    }

    fn validate_merkle_branch(shard: &Vec<u8>, proof: &Vec<Vec<u8>>) -> Vec<u8> {
        let mut hash = Sha256::digest(shard).to_vec();
        for sibling in proof {
            let combined = if hash < *sibling {
                [hash.clone(), sibling.clone()].concat()
            } else {
                [sibling.clone(), hash.clone()].concat()
            };
            hash = Sha256::digest(&combined).to_vec();
        }
        hash
    }

    async fn handle_propose(
        &self,
        sender: usize,
        root: Vec<u8>,
        proof: Vec<Vec<u8>>,
        shard: Vec<u8>,
        epoch_id: u64,
        transaction_size: usize,
        data_shards: usize,
    ) {
        info!("Node {}: Handling propose request from {}", self.id, sender);

        // Ensure no epoch overlap
        if let Err(e) = self.ensure_no_overlap(epoch_id).await {
            error!("Node {}: Epoch overlap detected for epoch {}: {}", self.id, epoch_id, e);
            return;
        }

        let max_shard_size = (transaction_size + data_shards - 1) / data_shards;

        if shard.len() > max_shard_size {
            info!(
                "Node {}: Proposal from Node {} rejected due to shard size. Shard size: {}, Max size: {}",
                self.id, sender, shard.len(), max_shard_size
            );
            return;
        }

        let computed_root = Self::validate_merkle_branch(&shard, &proof);
        if computed_root != root {
            info!(
                "Node {}: Invalid Merkle proof from Node {}. Provided root: {:?}, Computed root: {:?}",
                self.id, sender, root, computed_root
            );
            return;
        }

        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        info!(
            "Node {}: Proposal from Node {} accepted for epoch {}. Quorum votes for root {:?}: {}",
            self.id, sender, epoch_id, root, *counter
        );
    }

    async fn handle_prevote(&self, sender: usize, root: Vec<u8>) {
        info!("Node {}: Handling prevote request from {}", self.id, sender);

        let mut quorum_votes = self.quorum_votes.write().await;
        if let Some(counter) = quorum_votes.get_mut(&root) {
            *counter += 1;

            if *counter >= 2 * self.f() + 1 {
                info!(
                    "Node {}: Quorum reached for root {:?} with {} votes",
                    self.id, root, *counter
                );
            }
        } else {
            info!("Node {}: Prevote for unknown root {:?}", self.id, root);
        }
    }

    async fn handle_commit(&self, sender: usize, root: Vec<u8>) {
        info!("Node {}: Handling commit request from {}", self.id, sender);

        let quorum_votes = self.quorum_votes.read().await;
        if let Some(counter) = quorum_votes.get(&root) {
            if *counter >= 2 * self.f() + 1 {
                info!("Node {}: Commit finalized for root {:?}", self.id, root);
            } else {
                info!(
                    "Node {}: Insufficient votes for commit on root {:?}",
                    self.id, root
                );
            }
        } else {
            info!("Node {}: Commit for unknown root {:?}", self.id, root);
        }
    }

    async fn ensure_no_overlap(&self, epoch_id: u64) -> Result<(), &'static str> {
        let mut tracker = self.epoch_tracker.lock().await;
        if tracker.contains(&epoch_id) {
            Err("Epoch overlap detected")
        } else {
            tracker.insert(epoch_id);
            Ok(())
        }
    }
}

fn initialize_apis(node: Arc<Node>, config: Config) -> Router {
    Router::new()
        .route("/health", axum::routing::get({
            let node = node.clone();
            move || async move {
                let quorum_votes = node.quorum_votes.read().await;
                Json(Response {
                    status: format!("alive; quorum votes: {:?}", *quorum_votes),
                })
            }
        }))
        .route("/propose", post({
            let node = node.clone();
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                let transaction_size = config.consensus.transaction_size;
                let data_shards = config.consensus.data_shards;
                async move {
                    node.handle_propose(
                        payload.sender,
                        payload.root,
                        payload.proof,
                        payload.shard,
                        payload.epoch_id,
                        transaction_size,
                        data_shards,
                    )
                    .await;
                    Json(Response {
                        status: format!(
                            "Node {}: Propose accepted from Node {} for epoch {}",
                            node.id, payload.sender, payload.epoch_id
                        ),
                    })
                }
            }
        }))
        .route("/prevote", post({
            let node = node.clone();
            move |Json(payload): Json<PrevoteRequest>| {
                let node = node.clone();
                async move {
                    node.handle_prevote(payload.sender, payload.root).await;
                    Json(Response {
                        status: format!(
                            "Node {}: Prevote accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
        .route("/commit", post({
            let node = node.clone();
            move |Json(payload): Json<CommitRequest>| {
                let node = node.clone();
                async move {
                    node.handle_commit(payload.sender, payload.root).await;
                    Json(Response {
                        status: format!(
                            "Node {}: Commit accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
}

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    info!("Loaded configuration: {:?}", config);

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes));
    let app = initialize_apis(node, config);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
