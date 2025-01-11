use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock};


// Node structure
#[derive(Debug, Clone)]
pub struct Node {
pub id: usize,
pub total_nodes: usize,
pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
pub epoch_round_id: Arc<Mutex<HashSet<u64>>>,
pub proposal_tracker: Arc<Mutex<HashSet<usize>>>,
}

impl Node {
    pub fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            epoch_round_id: Arc::new(Mutex::new(HashSet::new())),
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3 // Fault tolerance
    }

}