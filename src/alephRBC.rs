use axum::{routing::post, Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    ip_manager_address: String,
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
    sender: usize,
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
    proposal_tracker: Arc<Mutex<HashSet<usize>>>,
}

impl Node {
    fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            epoch_tracker: Arc::new(Mutex::new(HashSet::new())),
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn f(&self) -> usize {
        (self.total_nodes - 1) / 3 // Fault tolerance
    }

    fn validate_merkle_branch(shard: &[u8], proof: &[Vec<u8>]) -> Vec<u8> {
        info!("Validating merkle branch");

        let mut hash = Sha256::digest(shard).to_vec();
        for sibling in proof {
            let combined = if hash < *sibling {
                [hash.clone(), sibling.clone()].concat()
            } else {
                [sibling.clone(), hash.clone()].concat()
            };
            hash = Sha256::digest(&combined).to_vec();
        }
        info!("Done validating merkle branch");
        hash
    }

    async fn ensure_no_overlap(&self, epoch_id: u64) -> Result<(), &'static str> {
        info!("Node {}: Detecting if there is overlap for epoch {}", self.id, epoch_id);
    
        let mut tracker = self.epoch_tracker.lock().await;
        if tracker.contains(&epoch_id) {
            info!("Node {}: Overlap for epoch {} detected, but continuing", self.id, epoch_id);
            Ok(())
        } else {
            tracker.insert(epoch_id);
            info!("Node {}: No overlap detected for epoch {}", self.id, epoch_id);
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
        info!("Node {}: ==== Handling propose request from Node {} ====", self.id, sender);
        
        let sync_result = self.handle_sync_epoch(epoch_id, sender).await;
        let no_overlap_result = self.ensure_no_overlap(epoch_id).await;
        let computed_root = Node::validate_merkle_branch(&shard, &proof);
        
        // Log individual failures explicitly
        if sync_result.is_err() {
            error!(
                "Node {}: Propose phase failed for epoch {} due to synchronization error: {:?}",
                self.id, epoch_id, sync_result.err().unwrap()
            );
            return;
        }
        
        if no_overlap_result.is_err() {
            error!(
                "Node {}: Propose phase failed for epoch {} due to overlap detection: {:?}",
                self.id, epoch_id, no_overlap_result.err().unwrap()
            );
            return;
        }
        
        if computed_root != root {
            error!(
                "Node {}: Propose phase failed for epoch {} due to Merkle root mismatch. Computed: {:?}, Expected: {:?}",
                self.id, epoch_id, computed_root, root
            );
        }
        
        // If everything is successful
        info!("Node {}: Propose phase successful for epoch {} from {}", self.id, epoch_id, sender);
    
        // Store the proposal for this epoch
        let mut proposal_tracker = self.proposal_tracker.lock().await;
        proposal_tracker.insert(sender);
    
        // Check if all proposals are received
        if proposal_tracker.len() == config.node.total_nodes -1 {
            info!("Node {}: All proposals received for epoch {}", self.id, epoch_id);

        // Synchronize epoch
        for node_url in &config.network.nodes {
            let payload = json!({ "epoch_id": epoch_id + 1 });
            if let Err(e) = client
                .post(format!("http://{}/sync_epoch", node_url))
                .json(&payload)
                .send()
                .await
            {
                error!("Failed to synchronize epoch with node {}: {:?}", node_url, e);
            } else {
                info!("Node {}: Synchronized epoch {} with {}", self.id, epoch_id + 1, node_url);
            }
        }

            let mut tracker = self.epoch_tracker.lock().await;
            tracker.insert(epoch_id + 1);  // Move to next epoch
            
            proposal_tracker.clear();
            // Transition to prevote phase
            self.handle_prevote(sender, root.clone(), epoch_id).await;
    
            // Broadcast prevote
            for node_url in &config.network.nodes {
                let payload = PrevoteRequest {
                    sender: self.id,
                    root: root.clone(),
                    epoch_id,
                };
                if let Err(e) = client.post(format!("http://{}/prevote", node_url))
                    .json(&payload)
                    .send()
                    .await
                {
                    error!("Failed to send prevote to node {}: {:?}", node_url, e);
                } else {
                    info!("Node {}: Prevote broadcasted to {}", self.id, node_url);
                }
            }
    
            // Clear tracker for next epoch
            proposal_tracker.clear();
        } else {
            info!(
                "Node {}: Waiting for more proposals for epoch {}. Received: {}",
                self.id, epoch_id, proposal_tracker.len()
            );
        }
    }
    
    

    async fn handle_prevote(&self, sender: usize, root: Vec<u8>, epoch_id: u64) {
        info!("Node {}: ==Handling== prevote request from Node {}", self.id, sender);

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
        info!("Node {}: LEAVING prevote request from Node {}", self.id, sender);
    }

    async fn handle_commit(&self, sender: usize, root: Vec<u8>) {
        info!("Node {}: ==Handling== commit request from Node {}", self.id, sender);

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

    async fn handle_sync_epoch(&self, epoch_id: u64, sender: usize) -> Result<(), &'static str> {
        info!("Node {}: Synchronizing epoch {} from {}", self.id, epoch_id,sender);

        let mut tracker = self.epoch_tracker.lock().await;
        if tracker.contains(&epoch_id) {
            info!("Node {}: Epoch {} already synchronized with {}", self.id, epoch_id, sender);
            Ok(())
        } else {
            tracker.insert(epoch_id);
            info!("Node {}: Epoch {} synchronized successfully with node {}", self.id, epoch_id,sender);
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
                info!("Node {}: ==== Handling synch epoch request from Node {} ====", node.id, payload.sender);
                async move {
                    match node.handle_sync_epoch(payload.epoch_id, payload.sender).await {
                        Ok(_) => Json(Response {
                            status: format!(
                                "Node {}: Epoch {} synchronized successfully from {}",
                                node.id, payload.epoch_id , payload.sender
                            ),
                        }),
                        Err(e) => Json(Response {
                            status: format!(
                                "Node {}: Failed to synchronize epoch {} from {}: {}",
                                node.id, payload.epoch_id,payload.sender,e
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
