use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::info;
use serde::Deserialize;
use reed_solomon_erasure::galois_8::ReedSolomon;
use ring::digest::{Context, SHA256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use toml;

const FAULT_TOLERANCE: usize = 1; // Fault tolerance threshold
const MINIMUM_SHARES: usize = FAULT_TOLERANCE + 1; // Minimum shares for quorum

/// DAG Node structure to track rounds and data
#[derive(Clone)]
struct DagNode {
    round: usize,
    data: Vec<u8>,
    parents: Vec<usize>, // Parent nodes in the DAG
}

/// Merkle Tree for cryptographic data integrity
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
    round: usize,
}

#[derive(Debug, Deserialize)]
struct NodeConfig {
    id: usize,
    total_nodes: usize,
}

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
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
    message_rx: mpsc::Receiver<Message>,
}

enum Message {
    Propose { sender: usize, chunk: Vec<u8>, proof: Vec<u8> },
    Prevote { sender: usize, root: Vec<u8> },
    Commit { sender: usize, root: Vec<u8> },
}

impl Node {
    fn new(config: Arc<Config>) -> (Arc<Mutex<Self>>, mpsc::Sender<Message>) {
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
            message_rx: rx,
        };
        (Arc::new(Mutex::new(node)), tx)
    }

    async fn run(node: Arc<Mutex<Self>>) {
        let node_clone = node.clone();
        let config = {
            let locked_node = node.lock().await;
            locked_node.config.clone()
        };

        tokio::spawn(async move {
            loop {
                let message = {
                    let mut locked_node = node_clone.lock().await;
                    locked_node.message_rx.recv().await
                };

                if let Some(message) = message {
                    let node_clone = node_clone.clone();
                    tokio::spawn(async move {
                        let locked_node = node_clone.lock().await;
                        match message {
                            Message::Propose { sender, chunk, proof } => {
                                locked_node.handle_propose(sender, chunk, proof).await;
                            }
                            Message::Prevote { sender, root } => {
                                locked_node.handle_prevote(sender, root).await;
                            }
                            Message::Commit { sender, root } => {
                                locked_node.handle_commit(sender, root).await;
                            }
                        }
                    });
                } else {
                    break;
                }
            }
        });

        for i in 0..config.consensus.batch_size {
            let data = vec![i as u8; config.consensus.transaction_size];
            let locked_node = node.lock().await;
            locked_node.propose(data).await;
        }
    }

    async fn propose(&self, data: Vec<u8>) {
        let rs = ReedSolomon::new(MINIMUM_SHARES, self.total_nodes - MINIMUM_SHARES)
            .expect("Failed to initialize Reed-Solomon encoder");

        let shard_size = (data.len() + MINIMUM_SHARES - 1) / MINIMUM_SHARES;
        let mut shards: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; rs.total_shard_count()];

        for (i, byte) in data.iter().enumerate() {
            shards[i % MINIMUM_SHARES][i / MINIMUM_SHARES] = *byte;
        }

        rs.encode(&mut shards).expect("Reed-Solomon encoding failed");

        let merkle_tree = MerkleTree::new(&shards, rs.total_shard_count());

        for (i, shard) in shards.into_iter().enumerate() {
            if let Some(proof) = merkle_tree.branch(i) {
                for peer in &self.config.network.nodes {
                    let url = format!("http://{}/propose", peer);
                    let payload = serde_json::json!({
                        "sender": self.id,
                        "chunk": shard,
                        "proof": proof,
                    });

                    let client = reqwest::Client::new();
                    if let Err(err) = client.post(&url).json(&payload).send().await {
                        info!(
                            "Node {} encountered an error sending shard to {}: {}",
                            self.id, peer, err
                        );
                    }
                }
            }
        }

        info!("Node {} completed proposal phase.", self.id);
    }

    async fn handle_propose(&self, _sender: usize, chunk: Vec<u8>, _proof: Vec<u8>) {
        let root = MerkleTree::hash_data(&chunk);
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

    async fn handle_prevote(&self, _sender: usize, root: Vec<u8>) {
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

    async fn handle_commit(&self, _sender: usize, root: Vec<u8>) {
        info!("Node {} committed to root {:?}", self.id, root);
    }
}

fn write_host_log(message: &str) -> std::io::Result<()> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let log_path = "/home/aleph-node/logs/node_status";
    let mut file = OpenOptions::new().append(true).create(true).open(log_path)?;
    writeln!(file, "[{}] {}", timestamp, message)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    if let Err(e) = write_host_log("AlephRBC rust code initialized.") {
        eprintln!("Failed to write to host log: {}", e);
    }

    let config = Arc::new(load_config("/home/aleph-node/aleph-node-config.toml"));
    let (node, tx) = Node::new(config.clone());

    tokio::spawn(async move {
        Node::run(node.clone()).await;
    });

    for _peer in &config.network.nodes {
        let _ = tx.send(Message::Propose {
            sender: config.node.id,
            chunk: vec![1, 2, 3],
            proof: vec![4, 5, 6],
        }).await;
    }
}
