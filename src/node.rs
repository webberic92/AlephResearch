use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use crate::structs::Node;

impl Node {
    pub fn new(id: usize, total_nodes: usize) -> Self {
        Self {
            id,
            total_nodes,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            epoch_tracker: Arc::new(Mutex::new(HashSet::new())),
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())), // Proper initialization
        }
    }
}