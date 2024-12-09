use axum::{routing::post, Json, Router};
use std::{error::Error, sync::Arc};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tracing::{info, error};
use serde::{Deserialize, Serialize};
use tracing_subscriber;

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


    // Receiver Logic:

    // What it Does:
    //     Accepts a propose request containing the root and validates the size of the root to prevent oversized proposals.
    //     Increments the quorum votes for the corresponding root if the size is valid.

    // Issues/Improvements:
    //     Validation of Merkle Branches:
    //         The receiver should validate the Merkle branch included in the propose message against the Merkle root.
    //     Lack of Context for Shares:
    //         The receiver logic does not handle shares or check their validity (e.g., reconstructing the data using erasure coding or verifying consistency with the root).

    // Missing Steps:
    //     Validate the received Merkle branch against the Merkle root.
    //     Add logic for handling shares, if necessary, in this phase.


    // Phase 1: Proposal Phase
    // The sender node creates shares of the data to be broadcast using erasure coding
    // and computes a Merkle tree root for the shares. Each share, along with the 
    // corresponding Merkle branch, is sent to the respective recipient nodes in a 
    // `propose` message. Nodes validate the size of the share to prevent malicious 
    // oversized proposals.   
    pub async fn handle_propose(&self, root: Vec<u8>, max_size: usize) {
        info!("Node {} handling propose request", self.id);

        if root.len() > max_size {
            info!(
                "Node {} rejected proposal due to size: {} (max: {}).",
                self.id,
                root.len(),
                max_size
            );
            return;
        }

        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;

        info!(
            "Node {} successfully handled propose. Quorum votes for root {:?}: {}",
            self.id, root, *counter
        );
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
}

fn initialize_apis(node: Arc<Node>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                async move {
                    node.handle_propose(payload.root, 1024).await;
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
    let app = initialize_apis(node);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
