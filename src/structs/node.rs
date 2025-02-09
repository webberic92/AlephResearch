use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock};
use tracing::{error, info};
use crate::handlers::handle_propose::handle_propose;
use super::requests::ProposeRequest;


/// **🔹 DAG Unit Structure**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagUnit {
    pub unit_id: u64,
    pub proposer_node: usize,
    pub data: Vec<u8>,
    pub parent_units: Vec<String>,
    pub finalization_timestamp: u64,
}



/// **📌 Node Struct: Represents a single node in the Aleph RBC protocol.**
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    pub current_epoch: Arc<Mutex<u64>>, // Tracks the current epoch explicitly
    pub proposal_tracker: Arc<Mutex<HashMap<u64, HashMap<usize, ProposeRequest>>>>,    // pub finalized_blocks: Arc<Mutex<HashSet<Vec<u8>>>>,
    pub dag: Arc<RwLock<HashMap<u64, Vec<DagUnit>>>>,
    pub ip_address: String,
    pub ip_manager_address: String,
    pub nodes: Vec<String>,
    pub proposal_sender: Sender<ProposeRequest>, // Proposal queue for async processing
}

impl Node {
    /// **🔹 Node Constructor: Initializes a new node and starts processing proposals asynchronously.**
    pub fn new(
        id: usize,
        total_nodes: usize,
        ip_address: String,
        nodes: Vec<String>,
        ip_manager_address: String,
        client: Arc<Client>,
    ) -> Arc<RwLock<Self>> {
        let (proposal_sender, proposal_receiver) = mpsc::channel(100);
        let node = Arc::new(RwLock::new(Self {
            id,
            total_nodes,
            ip_address,
            ip_manager_address,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            current_epoch: Arc::new(Mutex::new(1)), // Start at epoch 1
            proposal_tracker: Arc::new(Mutex::new(HashMap::new())), // Use HashMap for proposal storage
            // finalized_blocks: Arc::new(Mutex::new(HashSet::new())),
            dag: Arc::new(RwLock::new(HashMap::new())),
            nodes,
            proposal_sender,
        }));

        // Spawn a background task to process proposals
        let node_clone = Arc::clone(&node);
        tokio::spawn(async move {
            Node::process_proposals(node_clone, client, proposal_receiver).await;
        });

        node
    }

    /// **🔹 Asynchronous Proposal Processing**
    /// - Processes proposals as they arrive via the message queue.
    async fn process_proposals(
        node: Arc<RwLock<Node>>,
        client: Arc<Client>,
        mut receiver: Receiver<ProposeRequest>,
    ) {
        while let Some(propose_request) = receiver.recv().await {
            info!(
                "Node {}: Processing queued proposal for epoch {} from node {}",
                node.read().await.id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
            );

            if let Err(err) = handle_propose(node.clone(), client.clone(), propose_request).await {
                error!("Node {}: Failed to process proposal: {:?}", node.read().await.id, err);
            }
        }
    }

    /// **🔹 Compute Fault Tolerance Threshold (f)**
    pub fn get_fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3
    }

    /// **🔹 Compute Quorum Threshold**
    pub fn get_quorum_threshold(&self) -> usize {
        let f = self.get_fault_tolerance_threshold();
        let quorum = 2 * f + 1;
        info!(
            "Node {}: Total nodes: {}, Fault tolerance f: {}, Required quorum: {}",
            self.id, self.total_nodes, f, quorum
        );
        quorum
    }

    /// **🔹 Check if Quorum is Reached**
    pub async fn is_quorum_reached(&self, epoch_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold();
        let quorum_votes = self.quorum_votes.read().await;
        let vote_count = quorum_votes.get(&epoch_id.to_be_bytes().to_vec()).cloned().unwrap_or(0);

        info!(
            "Node {}: Checking quorum for epoch {}. Votes: {}, Threshold: {}",
            self.id, epoch_id, vote_count, quorum_threshold
        );

        vote_count >= quorum_threshold
    }

    /// **🔹 Update Proposal Tracker**
    /// - Stores received proposals and tracks count.
    pub async fn update_proposal_tracker(
        node: Arc<RwLock<Node>>,
        propose_request: ProposeRequest,
    ) -> Result<(usize, usize, Vec<ProposeRequest>), String> {
        let node_id = node.read().await.id;
        let epoch_id = propose_request.base.epoch_id;
        
        info!("Node {}: Acquiring write lock for proposal tracker update...", node_id);
    
        let proposal_count;
        let required_proposals;
        let stored_proposals;
    
        {
            let node_write = node.write().await;
            let mut proposal_tracker = node_write.proposal_tracker.lock().await;
    
            // ✅ Ensure there is a HashMap for the given epoch
            let epoch_entry = proposal_tracker.entry(epoch_id).or_insert_with(HashMap::new);
    
            // ✅ Store proposal in the epoch-specific tracker
            epoch_entry.insert(propose_request.base.proposing_node_id, propose_request.clone());
    
            proposal_count = epoch_entry.len();
            stored_proposals = epoch_entry.values().cloned().collect();
        } // 🔴 Drop write lock immediately
    
        {
            let node_read = node.read().await;
            let node_count = node_read.total_nodes;
            let f = node_read.get_fault_tolerance_threshold();
            required_proposals = node_count - f;
        } // 🔴 Drop read lock immediately
    
        info!(
            "Node {}: Proposal added from Node {} for epoch {}. Total proposals: {}. Required for consensus: {}.",
            node_id, propose_request.base.proposing_node_id, epoch_id, proposal_count, required_proposals
        );
    
        Ok((proposal_count, required_proposals, stored_proposals))
    }
    

  /// 🔹 **Get Last Unit ID in the DAG for a Given Epoch**
    /// - Retrieves the unit_id of the last entry in the DAG for `epoch_id`.
    pub async fn get_last_unit_id(&self, epoch_id: u64) -> Option<u64> {
        let dag_read = self.dag.read().await;
    
        // ✅ Check last unit in current epoch
        if let Some(units) = dag_read.get(&epoch_id) {
            if let Some(last_unit) = units.last() {
                info!(
                    "Node {}: Found last unit ID {} in epoch {}",
                    self.id, last_unit.unit_id, epoch_id
                );
                return Some(last_unit.unit_id);
            }
            info!("Node {}: No units found in epoch {}", self.id, epoch_id);
        }
    
        // ✅ Fall back to last finalized unit from previous epoch
        if epoch_id > 1 {
            if let Some(prev_units) = dag_read.get(&(epoch_id - 1)) {
                if let Some(last_unit) = prev_units.last() {
                    info!(
                        "Node {}: No units in epoch {}, falling back to last unit ID {} from epoch {}",
                        self.id, epoch_id, last_unit.unit_id, epoch_id - 1
                    );
                    return Some(last_unit.unit_id);
                }
            }
        }
    
        info!("Node {}: No previous units found, returning None", self.id);
        None // No units found
    }
    
    
    
    /// 🔹 **Get Next DAG Unit ID**
    /// - Gets the last unit ID for the current epoch and increments it.
    pub async fn get_next_dag_unit_id(&self, epoch_id: u64) -> u64 {
        if let Some(last_id) = self.get_last_unit_id(epoch_id).await {
            return last_id + 1;
        }
    
        // If epoch is empty, reference the last unit from the previous epoch
        if epoch_id > 1 {
            if let Some(last_id) = self.get_last_unit_id(epoch_id - 1).await {
                return last_id + 1;
            }
        }
    
        1 // Default to 1 for new DAG
    }
    
    
    /// 🔹 **Get Next Parent Units**
    /// - Retrieves parent units based on the last unit.
    pub async fn get_next_parents(&self, epoch_id: u64) -> Vec<String> {
        let dag_read = self.dag.read().await;
    
        // ✅ Get last unit(s) from the current epoch
        if let Some(units) = dag_read.get(&epoch_id) {
            if let Some(last_unit) = units.last() {
                let parents = vec![format!("{}", last_unit.unit_id)];
                info!(
                    "Node {}: Next parents for epoch {}: {:?}",
                    self.id, epoch_id, parents
                );
                return parents;
            }
        }
    
        // ✅ If no units exist, fall back to the last finalized unit of the previous epoch
        if epoch_id > 1 {
            if let Some(prev_units) = dag_read.get(&(epoch_id - 1)) {
                if let Some(last_unit) = prev_units.last() {
                    let parents = vec![format!("{}", last_unit.unit_id)];
                    info!(
                        "Node {}: No units in epoch {}, using parent {} from epoch {}",
                        self.id, epoch_id, last_unit.unit_id, epoch_id - 1
                    );
                    return parents;
                }
            }
        }
    
        info!("Node {}: No parent units found for epoch {}, returning empty list", self.id, epoch_id);
        Vec::new() // No parents if it's the first unit
    }
    
    
    pub async fn is_unit_committed(&self, parent_id: &str) -> bool {
        let dag_read = self.dag.read().await;
    
        // ✅ Handle first transaction (epoch 1): No parents to check
        if dag_read.is_empty() {
            info!("DAG is empty: Treating first transaction as committed.");
            return true;  // ✅ Allow the first transaction to commit
        }
    
        // 🔹 Iterate through all epochs in the DAG
        for (_epoch, units) in dag_read.iter() {
            // 🔹 Check if any unit references `parent_id` in `parent_units`
            if units.iter().any(|unit| unit.parent_units.contains(&parent_id.to_string())) {
                return true; // ✅ Parent unit was referenced in DAG → Committed
            }
        }
    
        false // ❌ Parent unit was not found → Not committed
    }

}
