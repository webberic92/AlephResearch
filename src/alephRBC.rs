use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tracing::{info, error};
use postgres::{Client, NoTls};

// Node represents each network node.
struct Node {
    id: usize,                    // Unique identifier
    total_nodes: usize,           // Total number of nodes
    fault_tolerance: usize,       // Number of Byzantine faults tolerated
    round: usize,                 // Current processing round
    received_propose: bool,       // Indicates if propose message was received
    storage: Arc<Mutex<HashMap<usize, Vec<u8>>>>, // Shared storage
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
        Node { id, total_nodes, fault_tolerance, round: 0, received_propose: false, storage }
    }

    async fn propose(&self, data: Vec<u8>) {
        let shares: Vec<Vec<u8>> = (0..self.total_nodes).map(|_| data.clone()).collect();
        let merkle_tree = MerkleTree::new(&data, self.total_nodes);
        for i in 0..self.total_nodes {
            if let Some(branch) = merkle_tree.branch(i) {
                self.send_propose(i, merkle_tree.root.clone(), branch, shares[i].clone()).await;
            }
        }
    }

    async fn send_propose(&self, to: usize, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} sends propose to {} with root {:?} and share {:?}", self.id, to, root, share);
    }

    async fn handle_propose(&mut self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        if self.received_propose || !self.check_size(&share) {
            return;
        }
        self.received_propose = true;
        self.wait_for_dag().await;
        self.multicast_prevote(root, branch, share).await;
    }

    fn check_size(&self, share: &Vec<u8>) -> bool {
        const SIZE_LIMIT: usize = 1024;
        share.len() <= SIZE_LIMIT
    }

    async fn wait_for_dag(&self) {
        info!("Node {} waiting for DAG to reach round {}", self.id, self.round - 1);
    }

    async fn multicast_prevote(&self, root: Vec<u8>, branch: Vec<u8>, share: Vec<u8>) {
        info!("Node {} multicasting prevote with root {:?} and share {:?}", self.id, root, share);
    }

    async fn handle_prevote(&self, root: Vec<u8>, shares: Vec<Vec<u8>>) {
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

    async fn multicast_commit(&self, root: Vec<u8>) {
        info!("Node {} multicasting commit with root {:?}", self.id, root);
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init(); // Initialize tracing

    let storage = Arc::new(Mutex::new(HashMap::new()));
    let nodes: Vec<Node> = (0..5).map(|id| Node::new(id, 5, 1, storage.clone())).collect();

    nodes[0].propose(vec![1, 2, 3, 4]).await; // Node 0 proposes a unit

    // Database connection to PostgreSQL
    match Client::connect("host=localhost user=postgres", NoTls) {
        Ok(mut client) => {
            client.batch_execute("
                CREATE TABLE IF NOT EXISTS metrics (
                    id SERIAL PRIMARY KEY,
                    node_id INTEGER,
                    metric_name TEXT,
                    value FLOAT
                )
            ").unwrap();
            info!("Connected to PostgreSQL and ensured metrics table exists.");
        },
        Err(e) => error!("Failed to connect to PostgreSQL: {}", e),
    }
}
