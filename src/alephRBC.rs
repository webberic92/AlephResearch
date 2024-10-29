use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tracing::info;
use std::time::{Instant, Duration};
use std::fs::{OpenOptions, File};
use std::io::Write;
use serde::Deserialize;
use std::fs;
use toml;



// Configuration struct for loading TOML configuration
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

// Load configuration from file
fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

// Node represents each network node
struct Node {
    id: usize,
    total_nodes: usize,
    fault_tolerance: usize,
    round: usize,
    received_propose: bool,
    storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
    message_count: usize,
    batch_size: usize,
    transaction_size: usize,
    transaction_metrics_log: String,
}

// MerkleTree handles data integrity checks
#[derive(Clone, Debug)]
struct MerkleTree {
    root: Vec<u8>,
    branches: HashMap<usize, Vec<u8>>,
}

impl MerkleTree {
    fn new(data: &[u8], n: usize) -> Self {
        let root = data.to_vec(); // Simplified Merkle root
        let branches = (0..n).map(|i| (i, data.to_vec())).collect();
        MerkleTree { root, branches }
    }

    fn branch(&self, index: usize) -> Option<Vec<u8>> {
        self.branches.get(&index).cloned()
    }
}

impl Node {
    fn new(config: &Config, fault_tolerance: usize, storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>) -> Self {
        Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            fault_tolerance,
            round: 0,
            received_propose: false,
            storage,
            message_count: 0,
            batch_size: config.consensus.batch_size,
            transaction_size: config.consensus.transaction_size,
            transaction_metrics_log: config.logging.transaction_metrics_log.clone(),
        }
    }

    async fn propose(&mut self, data: Vec<u8>) {
        let start_time = Instant::now();
        let shares: Vec<Vec<u8>> = self.erasure_code(data.clone());
        let merkle_tree = MerkleTree::new(&data, self.total_nodes);

        for i in 0..self.total_nodes {
            if let Some(branch) = merkle_tree.branch(i) {
                self.send_propose(i, merkle_tree.root.clone(), branch, shares[i].clone()).await;
            }
        }

        let duration = start_time.elapsed();
        self.log_throughput(duration, shares.len()).unwrap();
    }

    // Stub for erasure coding
    fn erasure_code(&self, data: Vec<u8>) -> Vec<Vec<u8>> {
        vec![data.clone(); self.total_nodes]
    }

    async fn send_propose(&mut self, to: usize, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} sends propose to {} with root {:?} and share {:?}", self.id, to, root, share);
        self.message_count += 1;
    }

    async fn handle_propose(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        if self.received_propose || !self.check_size(&share) {
            return;
        }
        self.received_propose = true;

        // Synchronize DAG and multicast prevote
        self.wait_for_dag(self.round - 1).await;
        self.multicast_prevote(root, branch, share).await;
    }

    fn check_size(&self, share: &Vec<u8>) -> bool {
        share.len() <= self.transaction_size
    }

    async fn wait_for_dag(&self, required_round: usize) {
        info!("Node {} waiting for DAG to reach round {}", self.id, required_round);
    }

    async fn multicast_prevote(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} multicasting prevote with root {:?} and share {:?}", self.id, root, share);
        self.message_count += 1;
    }

    async fn handle_prevote(&mut self, root: Vec<u8>, shares: Vec<Vec<u8>>) {
        let unit = shares.concat();
        if !self.is_valid_unit(&unit) {
            return;
        }
        self.wait_for_parents().await;
        self.multicast_commit(root).await;
    }

    fn is_valid_unit(&self, unit: &[u8]) -> bool {
        !unit.is_empty()
    }

    async fn wait_for_parents(&self) {
        info!("Node {} waiting for all parents to be received", self.id);
    }

    async fn multicast_commit(&mut self, root: Vec<u8>) {
        info!("Node {} multicasting commit with root {:?}", self.id, root);
        self.message_count += 1;
        self.log_communication_overhead().unwrap();
    }

    fn log_throughput(&self, duration: Duration, transaction_count: usize) -> std::io::Result<()> {
        let throughput = transaction_count as f64 / duration.as_secs_f64();
        let log_msg = format!("Node {}: Throughput = {:.2} transactions/sec over {} transactions.\n", self.id, throughput, transaction_count);
        self.write_to_log("throughput.log", log_msg)
    }

    fn log_latency(&self, latency: Duration) -> std::io::Result<()> {
        let log_msg = format!("Node {}: Consensus latency = {:.2?} seconds.\n", self.id, latency);
        self.write_to_log("latency.log", log_msg)
    }

    fn log_communication_overhead(&self) -> std::io::Result<()> {
        let log_msg = format!("Node {}: Total messages exchanged = {}.\n", self.id, self.message_count);
        self.write_to_log("communication_overhead.log", log_msg)
    }

    fn write_to_log(&self, file_name: &str, message: String) -> std::io::Result<()> {
        let log_path = format!("/home/aleph-node/logs/{}", file_name);
        let mut file = OpenOptions::new().append(true).create(true).open(log_path)?;
        file.write_all(message.as_bytes())?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let storage = Arc::new(Mutex::new(HashMap::new()));
    let mut nodes: Vec<Node> = (0..config.node.total_nodes)
        .map(|_| Node::new(&config, 1, storage.clone()))
        .collect();

    for node in &mut nodes {
        node.propose(vec![1, 2, 3, 4]).await;
    }
}
