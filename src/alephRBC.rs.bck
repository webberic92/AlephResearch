use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tracing::info;
use std::time::{Instant, Duration};
use std::fs::OpenOptions;
use std::io::Write;

// Node represents each network node.
struct Node {
    id: usize,                    // Unique identifier
    total_nodes: usize,           // Total number of nodes
    fault_tolerance: usize,       // Number of Byzantine faults tolerated
    round: usize,                 // Current processing round
    received_propose: bool,       // Indicates if propose message was received
    storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>, // Shared storage
    message_count: usize,         // Track communication overhead
}

// MerkleTree handles data integrity checks.
#[derive(Clone, Debug)]
struct MerkleTree {
    root: Vec<u8>,                // Merkle tree root hash
    branches: HashMap<usize, Vec<u8>>, // Branches per node for verification
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

// Node's behaviors and message handling.
impl Node {
    fn new(id: usize, total_nodes: usize, fault_tolerance: usize, storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>) -> Self {
        Node {
            id,
            total_nodes,
            fault_tolerance,
            round: 0,
            received_propose: false,
            storage,
            message_count: 0,
        }
    }

    async fn propose(&mut self, data: Vec<u8>) {
        let start_time = Instant::now();
        let shares: Vec<Vec<u8>> = (0..self.total_nodes).map(|_| data.clone()).collect();
        let merkle_tree = MerkleTree::new(&data, self.total_nodes);
        
        for i in 0..self.total_nodes {
            if let Some(branch) = merkle_tree.branch(i) {
                self.send_propose(i, merkle_tree.root.clone(), branch, shares[i].clone()).await;
            }
        }
        
        // Log transaction throughput
        let duration = start_time.elapsed();
        self.log_throughput(duration, shares.len()).unwrap();
    }

    async fn send_propose(&mut self, to: usize, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} sends propose to {} with root {:?} and share {:?}", self.id, to, root, share);
        self.message_count += 1; // Count message for communication overhead
    }

    async fn handle_propose(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        if self.received_propose || !self.check_size(&share) {
            return;
        }
        self.received_propose = true;
        let start_time = Instant::now();

        self.wait_for_dag().await;
        self.multicast_prevote(root, branch, share).await;

        let latency = start_time.elapsed();
        self.log_latency(latency).unwrap(); // Log latency for consensus completion
    }

    fn check_size(&self, share: &Vec<u8>) -> bool {
        const SIZE_LIMIT: usize = 1024;
        share.len() <= SIZE_LIMIT
    }

    async fn wait_for_dag(&self) {
        info!("Node {} waiting for DAG to reach round {}", self.id, self.round - 1);
    }

    async fn multicast_prevote(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} multicasting prevote with root {:?} and share {:?}", self.id, root, share);
        self.message_count += 1; // Count message for communication overhead
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
        self.message_count += 1; // Count message for communication overhead

        // Log communication overhead after the final commit
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
    tracing_subscriber::fmt::init(); // Initialize tracing

    let storage = Arc::new(Mutex::new(HashMap::new()));
    let mut nodes: Vec<Node> = (0..4).map(|id| Node::new(id, 4, 1, storage.clone())).collect();

    // Start the proposal for each node
    for node in &mut nodes {
        node.propose(vec![1, 2, 3, 4]).await;
    }
}
