use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tracing::info;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::fs::OpenOptions;
use std::io::Write;
use serde::Deserialize;
use std::fs;
use toml;

// Add the missing import for MerkleTree
struct MerkleTree {
    root: Vec<u8>,
    branches: HashMap<usize, Vec<u8>>,
}

impl MerkleTree {
    fn new(data: &[u8], n: usize) -> Self {
        let root = MerkleTree::hash_data(data);
        let branches = (0..n).map(|i| (i, data.to_vec())).collect();
        MerkleTree { root, branches }
    }

    fn branch(&self, index: usize) -> Option<Vec<u8>> {
        self.branches.get(&index).cloned()
    }

    fn hash_data(data: &[u8]) -> Vec<u8> {
        use ring::digest::{Context, SHA256}; // Import hashing functionality from ring
        let mut context = Context::new(&SHA256);
        context.update(data);
        context.finish().as_ref().to_vec()
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

#[derive(Debug, serde::Deserialize)]
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

fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

struct Node {
    id: usize,
    total_nodes: usize,
    fault_tolerance: usize,
    received_propose: bool,
    storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
    message_count: usize,
    batch_size: usize,
    transaction_size: usize,
}

impl Node {
    fn new(config: &Config, fault_tolerance: usize, storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>) -> Self {
        let node = Node {
            id: config.node.id,
            total_nodes: config.node.total_nodes,
            fault_tolerance,
            received_propose: false,
            storage,
            message_count: 0,
            batch_size: config.consensus.batch_size,
            transaction_size: config.consensus.transaction_size,
        };
        node.log_initialization();
        node
    }

    fn log_initialization(&self) {
        let log_msg = format!(
            "Node initialized with ID {} and total nodes {}\n",
            self.id, self.total_nodes
        );
        write_host_log(&log_msg).expect("Failed to log initialization");
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

    fn erasure_code(&self, data: Vec<u8>) -> Vec<Vec<u8>> {
        vec![data.clone(); self.total_nodes]
    }

    async fn send_propose(&mut self, to: usize, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} sends propose to {} with root {:?} and share {:?}", self.id, to, root, share);
        self.message_count += 1;
    }

    fn log_throughput(&self, duration: Duration, transaction_count: usize) -> std::io::Result<()> {
        let throughput = transaction_count as f64 / duration.as_secs_f64();
        let log_msg = format!(
            "Node {}: Throughput = {:.2} transactions/sec over {} transactions.\n",
            self.id, throughput, transaction_count
        );
        self.write_to_log("throughput.log", log_msg)
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

    if let Err(e) = write_host_log("Host initialization complete.") {
        eprintln!("Failed to write to host log: {}", e);
    }

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let storage = Arc::new(Mutex::new(HashMap::new()));
    let mut node = Node::new(&config, 1, storage.clone());

    let data = vec![0u8; config.consensus.transaction_size];
    node.propose(data).await;
}
