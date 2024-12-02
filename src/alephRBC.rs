use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use tracing::info;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::Deserialize;
use std::fs;
use toml;
use reed_solomon_erasure::galois_8::ReedSolomon;
use ring::digest::{Context, SHA256};

const FAULT_TOLERANCE: usize = 1;
const MINIMUM_SHARES: usize = FAULT_TOLERANCE + 1;
const CB: usize = 100; // Example batch-specific size parameter

#[derive(Clone)]
struct DagNode {
    round: usize,
    data: Vec<u8>,
    parents: Vec<usize>, // Tracking specific parent nodes
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

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

struct Node {
    id: usize,
    total_nodes: usize,
    fault_tolerance: usize,
    received_propose: HashMap<usize, bool>, // Track for each round
    storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
    quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    commit_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    dag: Arc<Mutex<HashMap<usize, DagNode>>>,
    round: usize,
    batch_size: usize,
    transaction_size: usize,
    termination_state: HashMap<usize, bool>, // Track per round
}

impl Node {
    fn new(config: &Config, fault_tolerance: usize, storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>) -> Self {
        let node = Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            fault_tolerance,
            received_propose: HashMap::new(),
            storage,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            commit_votes: Arc::new(RwLock::new(HashMap::new())),
            dag: Arc::new(Mutex::new(HashMap::new())),
            round: config.consensus.round,
            batch_size: config.consensus.batch_size,
            transaction_size: config.consensus.transaction_size,
            termination_state: HashMap::new(),
        };
        node.log_initialization();
        node
    }

    fn log_initialization(&self) {
        let log_msg = format!("Node initialized with ID {} and total nodes {}\n", self.id, self.total_nodes);
        write_host_log(&log_msg).expect("Failed to log initialization");
    }

    async fn propose(&mut self, data: Vec<u8>) {
        let shares = self.erasure_code(&data);
        let merkle_tree = MerkleTree::new(&shares, self.total_nodes);

        for i in 0..self.total_nodes {
            if let Some(branch) = merkle_tree.branch(i) {
                self.send_propose(i, merkle_tree.root.clone(), branch, shares[i].clone()).await;
            }
        }
    }

    fn erasure_code(&self, data: &[u8]) -> Vec<Vec<u8>> {
        let rs = ReedSolomon::new(MINIMUM_SHARES, self.total_nodes - MINIMUM_SHARES)
            .expect("Failed to create ReedSolomon instance");

        let mut shards: Vec<Vec<u8>> = vec![vec![0; data.len()]; self.total_nodes];
        for (i, shard) in shards.iter_mut().take(MINIMUM_SHARES).enumerate() {
            shard.copy_from_slice(data);
        }

        rs.encode(&mut shards).expect("Failed to encode data");
        shards
    }

    async fn send_propose(&mut self, to: usize, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} sends propose to {} with root {:?} and share {:?}", self.id, to, root, share);
        self.handle_prevote(root, branch, share).await;
    }

    async fn handle_prevote(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        if !*self.received_propose.entry(self.round).or_insert(false) && self.check_size(&share) {
            self.wait_for_round(self.round - 1).await;
            self.received_propose.insert(self.round, true);
            info!("Node {}: Prevote phase with root {:?}", self.id, root);
            self.register_vote(root.clone()).await;

            if self.check_quorum(&root).await {
                self.reconstruct_unit(&share).await;
                self.handle_commit(root).await;
            }
        }
    }

    async fn register_vote(&self, root: Vec<u8>) {
        let mut quorum_votes = self.quorum_votes.write().await;
        let counter = quorum_votes.entry(root).or_insert(0);
        *counter += 1;
    }

    async fn check_quorum(&self, root: &[u8]) -> bool {
        let quorum_votes = self.quorum_votes.read().await;
        let votes = quorum_votes.get(root).cloned().unwrap_or(0);
        votes >= (2 * self.fault_tolerance + 1) // Quorum condition
    }

    async fn reconstruct_unit(&mut self, share: &[u8]) {
        let unit_data = share.to_vec(); // Placeholder: reconstruct the data from shares
        let validity_check = self.validate_unit(&unit_data);
        if !validity_check {
            panic!("Node {}: Invalid unit after reconstruction", self.id);
        }
        info!("Node {} successfully reconstructed valid unit", self.id);
    }

    fn validate_unit(&self, unit: &[u8]) -> bool {
        unit.len() >= self.transaction_size
    }

    async fn handle_commit(&mut self, root: Vec<u8>) {
        self.wait_for_parents_output(vec![self.round - 1]).await;
        info!("Node {}: Entering commit phase with root {:?}", self.id, root);
        self.register_commit(root.clone()).await;

        if self.check_commit_threshold(&root).await {
            self.finalize_broadcast(root).await;
        }
    }

    async fn register_commit(&self, root: Vec<u8>) {
        let mut commit_votes = self.commit_votes.write().await;
        let counter = commit_votes.entry(root).or_insert(0);
        *counter += 1;
    }

    async fn check_commit_threshold(&self, root: &[u8]) -> bool {
        let commit_votes = self.commit_votes.read().await;
        commit_votes.get(root).cloned().unwrap_or(0) >= self.fault_tolerance + 1
    }

    async fn finalize_broadcast(&mut self, root: Vec<u8>) {
        info!("Node {}: Finalizing broadcast with root {:?}", self.id, root);
        let storage = self.storage.lock().await;
        if let Some(data) = storage.get(&self.id) {
            if MerkleTree::hash_data(data) == root {
                info!("Node {} successfully validated data integrity", self.id);
                self.termination_state.insert(self.round, true);
            } else {
                info!("Node {} detected Merkle root mismatch!", self.id);
            }
        }
    }

    fn check_size(&self, share: &[u8]) -> bool {
        share.len() <= self.transaction_size * CB
    }

    async fn wait_for_round(&self, required_round: usize) {
        loop {
            let dag = self.dag.lock().await;
            if dag.get(&required_round).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    

    async fn wait_for_parents_output(&self, parent_rounds: Vec<usize>) {
        for round in parent_rounds {
            self.wait_for_round(round).await;
        }
    }
}

fn write_host_log(message: &str) -> std::io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    let log_path = "/home/aleph-node/logs/node_status";
    let formatted_message = format!("[{}] {}\n", timestamp, message);
    let mut file = OpenOptions::new().append(true).create(true).open(log_path)?;
    file.write_all(formatted_message.as_bytes())?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    if let Err(e) = write_host_log("AlephRBC rust code initialized.") {
        eprintln!("Failed to write to host log: {}", e);
    }

    // Load configuration
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let storage = Arc::new(Mutex::new(HashMap::new()));
    let mut node = Node::new(&config, FAULT_TOLERANCE, storage.clone());

    // Use batch_size from the configuration for the number of transactions
    for i in 0..config.consensus.batch_size {
        let data = vec![i as u8; config.consensus.transaction_size]; // Sample data for each transaction
        node.propose(data).await;
    }

    info!("Node {} completed its {} transactions.", node.id, config.consensus.batch_size);
}
