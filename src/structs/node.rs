use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock};
use tracing::{error, info};
use crate::handlers::handle_propose::handle_propose;
use super::requests::ProposeRequest;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_id: String,
    pub data: Vec<u8>, // Transaction payload
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagUnit {
    pub unit_id: String,
    pub proposer_node: usize,
    pub round: u64,
    pub transactions: Vec<Transaction>, // ✅ Store multiple transactions
    pub parent_units: Vec<String>,
    pub merkle_root: String,
    pub finalization_timestamp: u64,
}




/// **📌 Node Struct: Represents a single node in the Aleph RBC protocol.**
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    pub current_round: Arc<Mutex<u64>>, // Tracks the current round explicitly
    pub proposal_tracker: Arc<Mutex<HashMap<u64, HashMap<usize, ProposeRequest>>>>,    // pub finalized_blocks: Arc<Mutex<HashSet<Vec<u8>>>>,
    pub dag: Arc<RwLock<HashMap<u64, Vec<DagUnit>>>>,
    pub ip_address: String,
    pub ip_manager_address: String,
    pub nodes: Vec<String>,
    pub proposal_sender: Sender<ProposeRequest>, // Proposal queue for async processing
    pub number_of_transactions: usize,
    pub transaction_size: usize,
    pub data_shards: usize,
}

impl Node {
    /// **🔹 Node Constructor: Initializes a new node and starts processing proposals asynchronously.**
    pub fn new(
        id: usize,
        total_nodes: usize,
        ip_address: String,
        nodes: Vec<String>,
        ip_manager_address: String,
        number_of_transactions: usize,
        transaction_size: usize,
        data_shards: usize,

    ) -> Arc<RwLock<Self>> {
        let (proposal_sender, proposal_receiver) = mpsc::channel(100);
        let node = Arc::new(RwLock::new(Self {
            id,
            total_nodes,
            ip_address,
            ip_manager_address,
            quorum_votes: Arc::new(RwLock::new(HashMap::new())),
            current_round: Arc::new(Mutex::new(1)), // Start at round 1
            proposal_tracker: Arc::new(Mutex::new(HashMap::new())), // Use HashMap for proposal storage
            // finalized_blocks: Arc::new(Mutex::new(HashSet::new())),
            dag: Arc::new(RwLock::new(HashMap::new())),
            nodes,
            proposal_sender,
            number_of_transactions,
            transaction_size,
            data_shards,
        }));

        // Spawn a background task to process proposals
        let node_clone = Arc::clone(&node);
        tokio::spawn(async move {
            Node::process_proposals(node_clone, proposal_receiver).await;
        });

        node
    }

    /// **🔹 Asynchronous Proposal Processing**
    /// - Processes proposals as they arrive via the message queue.
    async fn process_proposals(
        node: Arc<RwLock<Node>>,
        mut receiver: Receiver<ProposeRequest>,
    ) {
        while let Some(propose_request) = receiver.recv().await {
            info!(
                "Node {}: Processing queued proposal for round {} from node {}",
                node.read().await.id, propose_request.base.round_id, propose_request.base.proposing_node_id
            );

            if let Err(err) = handle_propose(node.clone(), propose_request).await {
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
    pub async fn is_quorum_reached(&self, round_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold();
        let quorum_votes = self.quorum_votes.read().await;
        let vote_count = quorum_votes.get(&round_id.to_be_bytes().to_vec()).cloned().unwrap_or(0);

        info!(
            "Node {}: Checking quorum for round {}. Votes: {}, Threshold: {}",
            self.id, round_id, vote_count, quorum_threshold
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
        let round_id = propose_request.base.round_id;
        
        info!("Node {}: Acquiring write lock for proposal tracker update...", node_id);
    
        let proposal_count;
        let required_proposals;
        let stored_proposals;
    
        {
            let node_write = node.write().await;
            let mut proposal_tracker = node_write.proposal_tracker.lock().await;
    
            // ✅ Ensure there is a HashMap for the given round
            let round_entry = proposal_tracker.entry(round_id).or_insert_with(HashMap::new);
    
            // ✅ Store proposal in the round-specific tracker
            round_entry.insert(propose_request.base.proposing_node_id, propose_request.clone());
    
            proposal_count = round_entry.len();
            stored_proposals = round_entry.values().cloned().collect();
        } // 🔴 Drop write lock immediately
    
        {
            let node_read = node.read().await;
            let node_count = node_read.total_nodes;
            let f = node_read.get_fault_tolerance_threshold();
            required_proposals = node_count - f;
        } // 🔴 Drop read lock immediately
    
        info!(
            "Node {}: Proposal added from Node {} for round {}. Total proposals: {}. Required for consensus: {}.",
            node_id, propose_request.base.proposing_node_id, round_id, proposal_count, required_proposals
        );
    
        Ok((proposal_count, required_proposals, stored_proposals))
    }
    

  /// 🔹 **Get Last Unit ID in the DAG for a Given round**
    /// - Retrieves the unit_id of the last entry in the DAG for `round_id`.
    pub async fn get_last_unit_id(&self, round_id: u64) -> Option<String> {
        let dag_read = self.dag.read().await;
    
        // ✅ First, check the last unit in the current round
        if let Some(units) = dag_read.get(&round_id) {
            if let Some(last_unit) = units.last() {
                info!(
                    "Node {}: Found last unit ID {} in round {}",
                    self.id, last_unit.unit_id, round_id
                );
                return Some(last_unit.unit_id.clone()); // ✅ Return unit_id as String
            }
            info!("Node {}: No units found in round {}", self.id, round_id);
        }
    
        // ✅ If no units exist in the current round, check the previous round
        if round_id > 1 {
            if let Some(prev_units) = dag_read.get(&(round_id - 1)) {
                if let Some(last_unit) = prev_units.last() {
                    info!(
                        "Node {}: No units in round {}, falling back to last unit ID {} from round {}",
                        self.id, round_id, last_unit.unit_id, round_id - 1
                    );
                    return Some(last_unit.unit_id.clone()); // ✅ Ensure String consistency
                }
            }
        }
    
        // ✅ If no units exist at all, return a default unit ID or None
        info!("Node {}: No previous units found, returning None", self.id);
        None
    }
    
    
    
    
    /// 🔹 **Get Next DAG Unit ID**
    /// - Gets the last unit ID for the current round and increments it.
    pub async fn get_next_dag_unit_id(&self, round_id: u64) -> String {
        if let Some(last_id) = self.get_last_unit_id(round_id).await {
            return format!("U{}", last_id.trim_start_matches('U').parse::<u64>().unwrap_or(0) + 1);
        }
    
        // ✅ If no units exist in the current round, check the previous round
        if round_id > 1 {
            if let Some(last_id) = self.get_last_unit_id(round_id - 1).await {
                return format!("U{}", last_id.trim_start_matches('U').parse::<u64>().unwrap_or(0) + 1);
            }
        }
    
        // ✅ Default to "U1" for a new DAG round
        "U1".to_string()
    }
    
    
    

    
    
    pub async fn is_unit_committed(&self, parent_id: &str) -> bool {
        let dag_read = self.dag.read().await;
    
        info!("Checking commitment for parent: {}", parent_id);
        info!("DAG state before commitment check: {:?}", *dag_read);
            
        // ✅ Handle first transaction (round 1): No parents to check
        if dag_read.is_empty() {
            info!("DAG is empty: Treating first transaction as committed.");
            return true;  // ✅ Allow the first transaction to commit
        }
    
        for (_round, units) in dag_read.iter() {
            if units.iter().any(|unit| unit.unit_id == parent_id) {  // ✅ Proper string comparison
                return true; // ✅ Check if the DAG contains this parent unit ID
            }
        }
        
        // ❌ Parent unit was not found
        false
    }
    

    pub async fn get_all_parents(&self, round_id: u64) -> Vec<String> {
        let dag_read = self.dag.read().await;
        let mut parents = Vec::new();
    
        // ✅ If there are already transactions in this round, use them as parents
        if let Some(units) = dag_read.get(&round_id) {
            if !units.is_empty() {
                parents.extend(units.iter().map(|unit| unit.unit_id.clone()));  // ✅ Return structured unit IDs
                info!(
                    "Node {}: Using units from current round {} as parents: {:?}",
                    self.id, round_id, parents
                );
                return parents; // 🔥 If current round has transactions, return immediately
            }
        }
    
        // ✅ If we are in round 1, return no parents
        if round_id == 1 {
            info!(
                "Node {}: Round 1 detected. No parents available.",
                self.id
            );
            return Vec::new();
        }
    
        // ✅ Otherwise, fallback to the last finalized transactions from the previous round
        if let Some(prev_units) = dag_read.get(&(round_id - 1)) {
            if !prev_units.is_empty() {
                parents.extend(prev_units.iter().map(|unit| unit.unit_id.clone()));  // ✅ Maintain String format
                info!(
                    "Node {}: Using previous round {} as parents: {:?}",
                    self.id, round_id - 1, parents
                );
            }
        }
    
        info!(
            "Node {}: Selected parents for round {} -> {:?}",
            self.id, round_id, parents
        );
    
        parents
    }
  
    

}
