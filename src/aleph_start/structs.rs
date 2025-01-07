use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync:: RwLock;
use std::sync::Arc;


// Configuration structures
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct Config {
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub node: NodeConfig,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct NetworkConfig {
    pub listen_address: String,
    pub nodes: Vec<String>,
    pub ip_manager_address: String, // Added for GTC APIs
    pub proposals: Vec<usize>,      // Add this line
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ConsensusConfig {
    pub transaction_size: usize,
    pub data_shards: usize,
    pub batch_size: usize,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct NodeConfig {
    pub id: usize,
    pub total_nodes: usize,
}

// Node structure
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
}

impl Node {
    pub fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}



