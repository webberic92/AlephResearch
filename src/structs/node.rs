use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock};
use tracing::info;
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    pub epoch_round_id: Arc<Mutex<HashSet<u64>>>,
    pub proposal_tracker: Arc<Mutex<HashSet<usize>>>,
    pub finalized_blocks: Arc<Mutex<HashSet<Vec<u8>>>>, // Renamed for clarity
    pub dag: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,   // DAG to store parent-child relationships
    pub ip_address: String,                           // Added for illustration
}

impl Node {
    pub fn new(id: usize, total_nodes: usize, ip_address: String) -> Self {
        Self {
            id,
            total_nodes,
            ip_address,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            epoch_round_id: Arc::new(Mutex::new(HashSet::new())),
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
            finalized_blocks: Arc::new(Mutex::new(HashSet::new())), // Updated name
            dag: Arc::new(RwLock::new(HashMap::new())),             // DAG initialization
        }
    }

    pub fn get_fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3 // f: Number of tolerable faults
    }
    
    pub fn get_quorum_threshold(&self) -> usize {
        2 * self.get_fault_tolerance_threshold() + 1 // 2f + 1: Quorum for consensus
    }

    pub async fn output_finalized_block(&self, block: Vec<u8>) {
        let mut finalized_blocks = self.finalized_blocks.lock().await;

        if !finalized_blocks.contains(&block) {
            finalized_blocks.insert(block.clone());
            info!("Node {}: Finalized block: {:?}", self.id, block);
        } else {
            info!("Node {}: Block {:?} is already finalized.", self.id, block);
        }
    }

    pub async fn is_unit_committed(&self, unit_ids: &[Vec<u8>]) -> bool {
        let finalized_blocks = self.finalized_blocks.lock().await;
        for unit_id in unit_ids {
            if !finalized_blocks.contains(unit_id) {
                return false;
            }
        }
        true
    }
}

