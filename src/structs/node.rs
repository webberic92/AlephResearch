use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use reqwest::Client;
use tokio::sync::{Mutex, RwLock, mpsc::{self, Sender, Receiver}};
use tracing::{error, info};
use crate::handlers::handle_propose::handle_propose;

use super::requests::ProposeRequest;

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
    pub ip_manager_address: String,                                                     
    pub nodes: Vec<String>,  
    pub proposal_sender: Sender<ProposeRequest>,  // 🔥 New proposal queue    
}

impl Node {
    /// **🔥 Node Constructor (Now Passes `node` and `client` to `process_proposals`)**
    pub fn new(
        id: usize,
        total_nodes: usize,
        ip_address: String,
        nodes: Vec<String>, 
        ip_manager_address: String,
        client: Arc<Client>, // ✅ Added Client here
    ) -> Arc<RwLock<Self>> {
        let (proposal_sender, proposal_receiver) = mpsc::channel(100);
        let node = Arc::new(RwLock::new(Self {
            id,
            total_nodes,
            ip_address,
            ip_manager_address,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            current_epoch: Arc::new(Mutex::new(1)), // Start with epoch ID 1
            proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
            finalized_blocks: Arc::new(Mutex::new(HashSet::new())), 
            dag: Arc::new(RwLock::new(HashMap::new())),             
            nodes,
            proposal_sender,
        }));

        let node_clone = Arc::clone(&node);
        tokio::spawn(async move {
            Node::process_proposals(node_clone, client, proposal_receiver).await;
        });

        node
    }

    /// **🔥 Separate Task for Handling Proposals Asynchronously**
    async fn process_proposals(
        node: Arc<RwLock<Node>>,
        client: Arc<Client>, 
        mut receiver: Receiver<ProposeRequest>
    ) {
        while let Some(propose_request) = receiver.recv().await {
            info!(
                "Processing queued proposal for epoch {} from node {}",
                propose_request.base.epoch_id, propose_request.base.proposing_node_id
            );

            // 🔥 **Call `handle_propose` while keeping it unchanged**
            if let Err(err) = handle_propose(node.clone(), client.clone(), propose_request).await {
                error!("Failed to process proposal: {:?}", err);
            }
        }
    }

    /// **🔥 Fault Tolerance Threshold Calculation**
    pub fn get_fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3 // f: Number of tolerable faults
    }
    
    /// **🔥 Quorum Calculation**
    pub fn get_quorum_threshold(&self) -> usize {
        let f = (self.total_nodes.saturating_sub(1)) / 3;
        let quorum = 2 * f + 1;
    
        info!(
            "Node {}: Total nodes: {}, Fault tolerance f: {}, Required quorum: {}",
            self.id, self.total_nodes, f, quorum
        );
    
        quorum
    }
    
    /// **🔥 Check if Quorum is Reached**
    pub async fn is_quorum_reached(&self, epoch_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold();
        let quorum_votes = self.quorum_votes.read().await;

        let vote_count = quorum_votes
            .get(&epoch_id.to_be_bytes().to_vec()) 
            .cloned() 
            .unwrap_or(0); 
            
        info!(
            "Node {}: Checking quorum for epoch {}. Votes: {}, Threshold: {}",
            self.id, epoch_id, vote_count, quorum_threshold
        );

        vote_count >= quorum_threshold
    }

    /// **🔥 Store Finalized Blocks**
    pub async fn output_finalized_block(&self, block: Vec<u8>) {
        let mut finalized_blocks = self.finalized_blocks.lock().await;

        if !finalized_blocks.contains(&block) {
            finalized_blocks.insert(block.clone());
            info!("Node {}: Finalized block: {:?}", self.id, block);
        } else {
            info!("Node {}: Block {:?} is already finalized.", self.id, block);
        }
    }

    /// **🔥 Check if a Set of Units is Committed**
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
