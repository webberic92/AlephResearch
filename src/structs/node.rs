use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex};
use tracing::{error, info};
use crate::handlers::handle_propose::handle_propose;
use super::requests::{DagUnit, ProposeRequest};


/// **📌 Node Struct: Represents a single node in the Aleph RBC protocol.**
#[derive(Debug)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<Mutex<HashMap<Vec<u8>, usize>>>,
    pub current_round: Arc<Mutex<u64>>, // Tracks the current epoch explicitly
    pub proposal_tracker: Arc<Mutex<HashMap<u64, HashMap<usize, ProposeRequest>>>>,
    pub dag: Arc<Mutex<HashMap<u64, Vec<DagUnit>>>>,
    pub ip_address: String,
    pub ip_manager_address: String,
    pub nodes: Vec<String>,
    pub proposal_sender: Sender<ProposeRequest>, // Proposal queue for async processing
    pub number_of_transactions: usize,
    pub transaction_size: usize,
    pub data_shards: usize,
    pub total_rounds: usize,
    pub commit_tracker: Arc<Mutex<std::collections::HashSet<String>>>, 
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
        total_rounds: usize,
    ) -> Arc<Mutex<Self>> {
        let (proposal_sender, proposal_receiver) = mpsc::channel(100);
        let node = Arc::new(Mutex::new(Self {
            id,
            total_nodes,
            ip_address,
            ip_manager_address,
            quorum_votes: Arc::new(Mutex::new(HashMap::new())),
            current_round: Arc::new(Mutex::new(1)), // Start at epoch 1
            proposal_tracker: Arc::new(Mutex::new(HashMap::new())), // Use HashMap for proposal storage
            dag: Arc::new(Mutex::new(HashMap::new())),
            nodes,
            proposal_sender,
            number_of_transactions,
            transaction_size,
            data_shards,
            total_rounds,
            commit_tracker: Arc::new(Mutex::new(std::collections::HashSet::new())),
            
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
        node: Arc<Mutex<Node>>,
        mut receiver: Receiver<ProposeRequest>,
    ) {
        while let Some(propose_request) = receiver.recv().await {
            let node_guard = node.lock().await;
            info!(
                "Node {}: Processing queued proposal for round {} from node {}",
                node_guard.id, propose_request.base.round_id, propose_request.base.proposing_node_id
            );

            // Unlock node before calling handle_propose to prevent deadlocks
            drop(node_guard);
            if let Err(err) = handle_propose(node.clone(), propose_request).await {
                let node_guard = node.lock().await;
                error!("Node {}: Failed to process proposal: {:?}", node_guard.id, err);
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
        2 * f + 1
    }

    /// **🔹 Check if Quorum is Reached**
    pub async fn is_quorum_reached(&self, round_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold();
        let vote_count = self.quorum_votes.lock().await.get(&round_id.to_be_bytes().to_vec()).cloned().unwrap_or(0);

        info!(
            "Node {}: Checking quorum for round {}. Votes: {}, Threshold: {}",
            self.id, round_id, vote_count, quorum_threshold
        );

        vote_count >= quorum_threshold
    }

    /// **🔄 Update Proposal Tracker**
    pub async fn update_proposal_tracker(
        node: Arc<Mutex<Node>>,
        propose_request: ProposeRequest,
    ) -> Result<(usize, usize, Vec<ProposeRequest>), String> {
        let node_guard = node.lock().await;
        let node_id = node_guard.id;
        let round_id = propose_request.base.round_id;

        tracing::info!("Node {}: Updating proposal tracker for round {}...", node_id, round_id);

        let proposal_count;
        let stored_proposals;

        {
            let mut tracker = node_guard.proposal_tracker.lock().await;
            let entry = tracker.entry(round_id).or_insert_with(HashMap::new);
            entry.insert(propose_request.base.proposing_node_id, propose_request.clone());
            proposal_count = entry.len();
            stored_proposals = entry.values().cloned().collect();
        }

        let required_proposals = node_guard.total_nodes - node_guard.get_fault_tolerance_threshold();
        tracing::info!(
            "Node {}: Added proposal for round {}. Count: {}/{}",
            node_id, round_id, proposal_count, required_proposals
        );

        Ok((proposal_count, required_proposals, stored_proposals))
    }

    /// **🔍 Get Last Unit ID in the DAG for a Given Round**
    /// **🔍 Get Last Unit ID in the DAG for a Given Round**
    pub async fn get_last_unit_id(&self, round_id: u64) -> Option<String> {
        let dag = self.dag.lock().await;
        dag.get(&round_id)
            .and_then(|units| units.last())
            .map(|unit| unit.unit_id.clone())
    }

    /// **🔢 Generate Next Unit ID with Dynamic Pattern (U<round>-<index>)**
    pub async fn get_next_dag_unit_id(&self, round_id: u64) -> String {
        if let Some(last_id) = self.get_last_unit_id(round_id).await {
            // Extract the index from the pattern "U<round>-<index>"
            let parts: Vec<&str> = last_id.split('-').collect();
            if parts.len() == 2 {
                if let Ok(index) = parts[1].parse::<u64>() {
                    return format!("U{}-{}", round_id, index + 1);
                }
            }
            // Fallback if parsing fails
            format!("U{}-1", round_id)
        } else {
            // No units for this round yet
            format!("U{}-1", round_id)
        }
    }

    /// **📋 Check if Parent Unit is Committed**
    pub async fn is_unit_committed(&self, parent_id: &str) -> bool {
        let dag = self.dag.lock().await;
    
        // 🌐 Log the entire DAG before searching
        tracing::info!("Node {}: Checking for parent unit: '{}'. Current DAG: {:?}", self.id, parent_id, *dag);
    
        let result = dag.values()
            .flatten()
            .any(|unit| unit.unit_id == parent_id);
    
        tracing::info!(
            "Node {}: Parent unit '{}' committed status: {}",
            self.id,
            parent_id,
            result
        );
    
        result
    }

    /// **🔗 Retrieve All Parents for a Round**
    /// **🔗 Retrieve All Parents for the Previous Round**
    pub async fn get_all_parents(&self, round_id: u64) -> Vec<String> {
        if round_id == 1 {
            // First round has no parents
            return Vec::new();
        }

        let dag_snapshot = {
            let dag = self.dag.lock().await;
            dag.clone()
        };

        // 🔍 Fetch units from the previous round
        dag_snapshot.get(&(round_id - 1))
            .map(|units| units.iter().map(|u| u.unit_id.clone()).collect())
            .unwrap_or_else(Vec::new)
    }

    
}
