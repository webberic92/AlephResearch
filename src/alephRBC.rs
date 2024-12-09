use axum::{routing::post, Json, Router};
use std::{collections::HashMap, fs, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tracing::{info, error};
use serde::{Deserialize, Serialize};
use tracing_subscriber;
use reed_solomon_erasure::galois_8::ReedSolomon;
use sha2::{Digest, Sha256};

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
    proof: Vec<Vec<u8>>, // Add proof (Merkle branch)
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
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    commit_votes: Arc<RwLock<usize>>,
}

impl Node {
    fn new(id: usize) -> Self {
        Self {
            id,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(0)),
        }
    }

    // Phase 1: Proposal Phase
    // The sender node creates shares of the data to be broadcast using erasure coding
    // and computes a Merkle tree root for the shares. Each share, along with the 
    // corresponding Merkle branch, is sent to the respective recipient nodes in a 
    // `propose` message. Nodes validate the size of the share to prevent malicious 
    // oversized proposals. They also validate the Merkle branch against the root.
    pub async fn handle_propose(
        &self,
        root: Vec<u8>,
        proof: Vec<Vec<u8>>,
        shard: Vec<u8>,
        max_transaction_size: usize,
    ) {
        info!("Node {} handling propose request", self.id);

        // Step 1: Validate shard size
        // Step 1: Validate shard size
        let data_shards = 4; // Number of data shards used in erasure coding
        let max_shard_size = (max_transaction_size + data_shards - 1) / data_shards; // Compute max shard size

        // Validate shard size against max_shard_size
        if shard.len() > max_shard_size {
            info!(
                "Node {} rejected proposal due to shard size: {} (max shard size: {}).",
                self.id,
                shard.len(),
                max_shard_size
            );
            return;
        }

        // Step 2: Validate Merkle branch against the root
        let computed_root = Self::validate_merkle_branch(&shard, &proof);
        if computed_root != root {
            info!(
                "Node {} rejected proposal due to invalid Merkle proof. Provided root: {:?}, Computed root: {:?}",
                self.id, root, computed_root
            );
            return;
        }

        // Step 3: Quorum vote logic
        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        info!(
            "Node {} successfully handled propose. Quorum votes for root {:?}: {}",
            self.id, root, *counter
        );
    }

    // Helper function to validate the Merkle branch
    fn validate_merkle_branch(shard: &Vec<u8>, proof: &Vec<Vec<u8>>) -> Vec<u8> {
        let mut hash = Sha256::digest(shard).to_vec(); // Hash the shard
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

    // Phase 2: Prevote Phase
    // Nodes validate the root from the propose phase and vote to move forward.
    async fn handle_prevote(&self) {
        info!("Node {} handling prevote request", self.id);
        let mut commit_votes = self.commit_votes.write().await;
        *commit_votes += 1;
    }

    // Phase 3: Commit Phase
    // Once a quorum of prevotes is reached, nodes finalize the transaction.
    async fn handle_commit(&self) {
        info!("Node {} handling commit request", self.id);
    }
}

fn initialize_apis(node: Arc<Node>, config: &Config) -> Router {
    let transaction_size = config.consensus.transaction_size;
    let data_shards = 4; // Adjust this if needed, but it's typically constant.
    let shard_size = (transaction_size + data_shards - 1) / data_shards;

    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let config = config.clone();
            let max_size: usize = config.consensus.transaction_size;
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                async move {
                    node.handle_propose(payload.root, payload.proof, payload.shard, max_size).await;
                    Json(Response {
                        status: "Propose accepted".to_string(),
                    })
                }
            }
        }))
        .route("/prevote", post({
            let node = node.clone();
            move |Json(_payload): Json<PrevoteRequest>| {
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
            move |Json(_payload): Json<CommitRequest>| {
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

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    info!("Loaded configuration: {:?}", config);

    let node = Arc::new(Node::new(config.node.id));
    let app = initialize_apis(node, &config);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
