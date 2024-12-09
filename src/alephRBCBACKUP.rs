use axum::{
    routing::post,
    Json, Router,
};
use std::{error::Error, sync::Arc};
use std::net::SocketAddr;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tracing::{info, error};
use tracing_subscriber;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
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
    transaction_count: Arc<RwLock<usize>>
 }

impl Node {
    fn new(config: Arc<Config>) -> Self {
        Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(HashMap::new())),
            config,
            transaction_count: Arc::new(RwLock::new(0))
        }
    }


    async fn process_transactions(&self) {
        let client = Client::new();
    
        info!(
            "Node {} starting proposal phase. Batch size: {}, Transaction size: {} bytes",
            self.id, self.config.consensus.batch_size, self.config.consensus.transaction_size
        );
    
        for i in 0..self.config.consensus.batch_size {
            // Create transaction data with the correct size
            let base_transaction = format!("Transaction {} from Node {}", i + 1, self.id);
            let transaction_size = self.config.consensus.transaction_size;
    
            // Adjust the transaction size
            let transaction_data = if base_transaction.len() >= transaction_size {
                base_transaction[..transaction_size].to_string() // Truncate if too large
            } else {
                let padding = "x".repeat(transaction_size - base_transaction.len());
                format!("{}{}", base_transaction, padding) // Pad if too small
            };
    
            // Broadcast proposal to all nodes
            for peer in &self.config.network.nodes {
                info!("Node {} sending proposal to {}", self.id, peer);
    
                match client
                    .post(format!("http://{}/propose", peer))
                    .json(&serde_json::json!({
                        "sender": self.id,
                        "data": transaction_data,
                        "round": i
                    }))
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            info!("Node {} proposal accepted by {}", self.id, peer);
                        } else {
                            error!("Node {} received error from {}", self.id, peer);
                        }
                    }
                    Err(e) => {
                        error!("Node {} failed to send proposal to {}: {:?}", self.id, peer, e);
                    }
                }
            }
        }
    
        info!("Node {} completed all proposals.", self.id);
    
        // Trigger the next phases (Prevote, Commit, Output)
        self.trigger_next_phases(client).await;
    }
    
// Trigger the next phases: Prevote, Commit, and Output
async fn trigger_next_phases(&self, client: Client) {
    info!("Node {} starting prevote phase.", self.id);
    // Prevote Phase: Simulate prevote multicast
    for peer in &self.config.network.nodes {
        match client
            .post(format!("http://{}/prevote", peer))
            .json(&serde_json::json!({
                "sender": self.id,
                "round": 1
            }))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Node {} prevote accepted by {}", self.id, peer);
                } else {
                    error!("Node {} prevote error from {}", self.id, peer);
                }
            }
            Err(e) => {
                error!("Node {} failed to send prevote to {}: {:?}", self.id, peer, e);
            }
        }
    }

    // Commit Phase: Simulate commit multicast
    info!("Node {} starting commit phase.", self.id);
    for peer in &self.config.network.nodes {
        match client
            .post(format!("http://{}/commit", peer))
            .json(&serde_json::json!({
                "sender": self.id,
                "round": 1
            }))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Node {} commit accepted by {}", self.id, peer);
                } else {
                    error!("Node {} commit error from {}", self.id, peer);
                }
            }
            Err(e) => {
                error!("Node {} failed to send commit to {}: {:?}", self.id, peer, e);
            }
        }
    }

    // Output Phase: Finalize and log the output
    info!("Node {} starting output phase.", self.id);
    info!("Node {} completed all phases of ch-RBC.", self.id);
}




    // Phase 1: Proposal Phase
    // The sender node creates shares of the data to be broadcast using erasure coding
    // and computes a Merkle tree root for the shares. Each share, along with the 
    // corresponding Merkle branch, is sent to the respective recipient nodes in a 
    // `propose` message. Nodes validate the size of the share to prevent malicious 
    // oversized proposals.
    async fn handle_propose(&self, root: Vec<u8>) {
        info!("handle_propose request");

        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root.clone()).or_insert(0);
        *counter += 1;
        info!("Node {} received propose. Quorum: {}", self.id, counter);
    }

    // Phase 2: Prevote Phase
    // Upon receiving a valid `propose` message, each node multicasts a `prevote` 
    // message to confirm the validity of the received data. Nodes wait until their 
    // local DAG has reached the appropriate round before broadcasting `prevote`, 
    // ensuring data synchronization across the network.
    async fn handle_prevote(&self, root: Vec<u8>) {
        let mut commit_votes = self.commit_votes.write().await;
        let counter = commit_votes.entry(root.clone()).or_insert(0);
        *counter += 1;
        info!("Node {} received prevote. Commit count: {}", self.id, counter);
    }


    // Phase 3: Commit Phase
    // After collecting `2f + 1` valid `prevote` messages, nodes reconstruct the 
    // original data from the received shares and validate it. If valid, and all 
    // parent data in the DAG has been received, the node multicasts a `commit` 
    // message. Nodes also forward received `commit` messages to ensure propagation.
    async fn handle_commit(&self, root: Vec<u8>) {
        info!("Node {} received commit for root {:?}", self.id, root);
    }

    // Phase 4: Output Phase TBD
    // Once a node collects `2f + 1` `commit` messages, it finalizes and outputs 
    // the reconstructed data, ensuring reliable broadcast across the network.
    async fn handle_Output(&self, root: Vec<u8>) {
        info!("This is the output phase. Node {} received output for root {:?}", self.id, root);
    }


}

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// HTTP Handlers
async fn propose_handler(Json(payload): Json<ProposeRequest>, node: Arc<RwLock<Node>>) -> Json<Response> {
    info!("Recieved propose request");

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

/// Initializes and runs the API server for the application.
/// This function sets up the API routes, parses the listen address,
/// binds the listener, and starts serving the application.
/// # Arguments
/// - `node`: The shared `Node` instance.
/// - `config`: The shared configuration.
fn initialize_apis(node: Arc<RwLock<Node>>, config: Arc<Config>) -> Router {
    Router::new()
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
        })
}



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration
    let config = Arc::new(load_config("/home/aleph-node/aleph-node-config.toml"));
    let node = Arc::new(RwLock::new(Node::new(config.clone())));

    // Initialize APIs
    let app = initialize_apis(node.clone(), config.clone());

    // Bind the listener manually
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    let listener = TcpListener::bind(addr).await?;
    info!("Successfully bound to {}", addr);

    // Start the Axum server with a manually bound listener
    let server_task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app.into_make_service()).await {
            error!("Server task failed: {:?}", e);
            Err::<(), Box<dyn std::error::Error + Send + Sync>>(Box::new(e))
        } else {
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
    });

    // Spawn the transaction processing task
    let transaction_task = tokio::spawn({
        let node = node.clone();
        async move {
            info!("Starting transaction processing...");
            node.read().await.process_transactions().await;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
    });

    // Use tokio::select! to wait for both tasks
    tokio::select! {
        server_result = server_task => {
            match server_result {
                Ok(Ok(())) => info!("Server task completed successfully."),
                Ok(Err(e)) => {
                    error!("Server task encountered an error: {:?}", e);
                    return Err(e);
                }
                Err(join_err) => {
                    error!("Server task failed to run: {:?}", join_err);
                    return Err(Box::new(join_err));
                }
            }
        }
        transaction_result = transaction_task => {
            match transaction_result {
                Ok(Ok(())) => info!("Transaction processing task completed successfully."),
                Ok(Err(e)) => {
                    error!("Transaction processing task encountered an error: {:?}", e);
                    return Err(e);
                }
                Err(join_err) => {
                    error!("Transaction processing task failed to run: {:?}", join_err);
                    return Err(Box::new(join_err));
                }
            }
        }
    }

    Ok(())
}


