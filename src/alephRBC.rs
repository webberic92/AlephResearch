use axum::{routing::post, Json, Router};
use reqwest::Client;
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

#[derive(Deserialize, Serialize)]
struct PrevoteRequest {
    sender: usize,
    root: Vec<u8>,
    epoch_id: u64,
}

#[derive(Deserialize)]
struct CommitRequest {
    sender: usize,
    root: Vec<u8>,
}

#[derive(Deserialize)]
struct SyncEpochRequest {
    epoch_id: u64,
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
        (self.total_nodes - 1) / 3 // Fault tolerance
    }

    fn validate_merkle_branch(shard: &[u8], proof: &[Vec<u8>]) -> Vec<u8> {
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

    async fn ensure_no_overlap(&self, epoch_id: u64) -> Result<(), &'static str> {
        let mut tracker = self.epoch_tracker.lock().await;
        if tracker.contains(&epoch_id) {
            Err("Epoch overlap detected")
        } else {
            tracker.insert(epoch_id);
            Ok(())
        }
    }

    async fn handle_propose(
        &self,
        client: &Client,
        config: &Config,
        sender: usize,
        root: Vec<u8>,
        proof: Vec<Vec<u8>>,
        shard: Vec<u8>,
        epoch_id: u64,
    ) {
        info!("Node {}: Handling propose request from Node {}", self.id, sender);

        if let Err(e) = self.handle_sync_epoch(epoch_id).await {
            error!(
                "Node {}: Failed to synchronize epoch {}. Error: {}",
                self.id, epoch_id, e
            );
            return;
        }

        if let Err(e) = self.ensure_no_overlap(epoch_id).await {
            error!("Node {}: Epoch overlap detected for epoch {}: {}", self.id, epoch_id, e);
            return;
        }

        let computed_root = Node::validate_merkle_branch(&shard, &proof);
        if computed_root != root {
            error!(
                "Node {}: Merkle proof invalid for epoch {}. Computed root: {:?}, Provided root: {:?}",
                self.id, epoch_id, computed_root, root
            );
            return;
        }

        // Trigger local prevote logic
        self.handle_prevote(sender, root.clone(), epoch_id).await;

        // Broadcast prevote to other nodes
        for node_url in &config.network.nodes {
            let payload = PrevoteRequest {
                sender: self.id,
                root: root.clone(),
                epoch_id,
            };
            if let Err(e) = client
                .post(format!("http://{}/prevote", node_url))
                .json(&payload)
                .send()
                .await
            {
                error!("Failed to send prevote to node {}: {:?}", node_url, e);
            } else {
                info!("Node {}: Prevote broadcasted to {}", self.id, node_url);
            }
        }
    }

    async fn handle_prevote(&self, sender: usize, root: Vec<u8>, epoch_id: u64) {
        info!("Node {}: Handling prevote request from Node {}", self.id, sender);

        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        if *counter >= 2 * self.f() + 1 {
            info!(
                "Node {}: Quorum reached for root {:?} with {} votes",
                self.id, root, *counter
            );
            self.handle_commit(sender, root).await;
        } else {
            info!(
                "Node {}: Prevote accepted for root {:?}, current votes: {}",
                self.id, root, *counter
            );
        }
    }

    async fn handle_commit(&self, sender: usize, root: Vec<u8>) {
        info!("Node {}: Handling commit request from Node {}", self.id, sender);

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

    async fn handle_sync_epoch(&self, epoch_id: u64) -> Result<(), &'static str> {
        info!("Node {}: Synchronizing epoch {}", self.id, epoch_id);

        let mut tracker = self.epoch_tracker.lock().await;
        if tracker.contains(&epoch_id) {
            Err("Epoch already synchronized")
        } else {
            tracker.insert(epoch_id);
            info!("Node {}: Epoch {} synchronized successfully", self.id, epoch_id);
            Ok(())
        }
    }
}

fn initialize_apis(node: Arc<Node>, config: Config, client: Arc<Client>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            let config = Arc::new(config);
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                let client = client.clone();
                let config = config.clone();
                async move {
                    node.handle_propose(
                        &client,
                        &config,
                        payload.sender,
                        payload.root,
                        payload.proof,
                        payload.shard,
                        payload.epoch_id,
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
                    node.handle_prevote(payload.sender, payload.root, payload.epoch_id).await;
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
        .route("/sync_epoch", post({
            let node = node.clone();
            move |Json(payload): Json<SyncEpochRequest>| {
                let node = node.clone();
                async move {
                    match node.handle_sync_epoch(payload.epoch_id).await {
                        Ok(_) => Json(Response {
                            status: format!(
                                "Node {}: Epoch {} synchronized successfully",
                                node.id, payload.epoch_id
                            ),
                        }),
                        Err(e) => Json(Response {
                            status: format!(
                                "Node {}: Failed to synchronize epoch {}: {}",
                                node.id, payload.epoch_id, e
                            ),
                        }),
                    }
                }
            }
        }))
        .route("/health", axum::routing::get({
            let node = node.clone();
            move || async move {
                let quorum_votes = node.quorum_votes.read().await;
                let epoch_tracker = node.epoch_tracker.lock().await;

                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, Epochs: {:?}",
                        node.id, *quorum_votes, *epoch_tracker
                    ),
                })
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
    let client = Arc::new(Client::new());

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes));
    let app = initialize_apis(node, config, client);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
