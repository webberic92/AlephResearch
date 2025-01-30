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
    pub current_epoch: Arc<Mutex<u64>>,          // Tracks the current epoch explicitly
    pub proposal_tracker: Arc<Mutex<HashSet<usize>>>,
    pub finalized_blocks: Arc<Mutex<HashSet<Vec<u8>>>>, 
    pub dag: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,   
    pub ip_address: String,                           
    pub nodes: Vec<String>,                                 // Added for illustration
}

impl Node {
    pub fn new(
        id: usize,
        total_nodes: usize,
        ip_address: String,
        nodes: Vec<String>, 
    ) -> Self {
        Self {
            id,
            total_nodes,
            ip_address,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            current_epoch: Arc::new(Mutex::new(1)), // Start with epoch ID 1
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
            finalized_blocks: Arc::new(Mutex::new(HashSet::new())), 
            dag: Arc::new(RwLock::new(HashMap::new())),             
            nodes,
        }
    }

    pub fn get_fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3 // f: Number of tolerable faults
    }
    
    pub fn get_quorum_threshold(&self) -> usize {
        2 * self.get_fault_tolerance_threshold() + 1 // 2f + 1: Quorum for consensus
    }
    
    pub async fn is_quorum_reached(&self, epoch_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold(); // Calculate quorum
        let quorum_votes = self.quorum_votes.read().await; // Lock quorum votes for read access

        let vote_count = quorum_votes
            .get(&epoch_id.to_be_bytes().to_vec()) // Get vote count for the epoch
            .cloned() // Clone the value to avoid holding the lock
            .unwrap_or(0); // Default to 0 if no votes exist

        vote_count >= quorum_threshold // Check if vote count satisfies the quorum
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

