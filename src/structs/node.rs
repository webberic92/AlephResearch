use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicU64, Arc},
};
use num_bigint::BigInt;
use tokio::sync::{mpsc::{self}, Mutex};
use tracing::info;
use crate::{processors::rbc_processor::RBCProcessor, utils::round_manager::round_manager_task};
use super::requests::{CommitRequest, DagUnit, ProposeRequest};
use crate::utils::events::Event;

/// **Events to notify the Round Manager**


/// **📌 Node Struct: Represents a single node in the Aleph RBC protocol.**
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<Mutex<HashMap<Vec<u8>, HashSet<String>>>>,
    pub current_round: Arc<Mutex<u64>>,
    pub proposal_tracker: Arc<Mutex<HashMap<u64, HashMap<usize, ProposeRequest>>>>,
    pub dag: Arc<Mutex<HashMap<u64, Vec<DagUnit>>>>,
    pub ip_address: String,
    pub ip_manager_address: String,
    pub nodes: Vec<String>,
    pub number_of_transactions: usize,
    pub transaction_size: usize,
    pub data_shards: usize,
    pub total_rounds: usize,
    pub commit_tracker: Arc<Mutex<HashMap<u64, Vec<CommitRequest>>>>,
    pub event_sender: mpsc::Sender<Event>,
    pub rbc_processor: Option<Arc<RBCProcessor>>,
    pub message_count: Arc<AtomicU64>,
    pub hash_to_prime_cache: Arc<Mutex<HashMap<u64, HashMap<String, BigInt>>>>,
    pub proposal_locks: Arc<Mutex<HashSet<u64>>>,
}

impl Node {
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
        let (event_sender, event_receiver) = mpsc::channel(100);
        let node = Arc::new(Mutex::new(Self {
            id,
            total_nodes,
            ip_address,
            ip_manager_address,
            quorum_votes: Arc::new(Mutex::new(HashMap::new())),
            current_round: Arc::new(Mutex::new(1)),
            proposal_tracker: Arc::new(Mutex::new(HashMap::new())),
            dag: Arc::new(Mutex::new(HashMap::new())),
            nodes,
            number_of_transactions,
            transaction_size,
            data_shards,
            total_rounds,
            commit_tracker: Arc::new(Mutex::new(HashMap::new())),
            event_sender,
            rbc_processor: None,
            message_count: Arc::new(AtomicU64::new(0)),
            hash_to_prime_cache: Arc::new(Mutex::new(HashMap::new())),
            proposal_locks: Arc::new(Mutex::new(HashSet::new())),
        }));

        let node_clone = node.clone();
        tokio::spawn(async move {
            if let Err(e) = round_manager_task(node_clone, event_receiver).await {
                tracing::error!("RoundManager encountered an error: {:?}", e);
            }
        });

        node
    }


    // pub fn get_message_count(&self) -> u64 {
    //     self.message_count.load(Ordering::Relaxed)
    // }
    
    /// **✅ Set `rbc_processor` After Initialization**
    pub async fn set_rbc_processor(node: Arc<Mutex<Node>>, rbc_processor: Arc<RBCProcessor>) {
        let mut node_guard = node.lock().await;
        node_guard.rbc_processor = Some(rbc_processor.clone());

        // ✅ Restart Round Manager with `rbc_processor`
        let event_receiver = mpsc::channel(100).1;
        let node_clone = node.clone();
        tokio::spawn(async move {
            if let Err(e) = round_manager_task(node_clone, event_receiver).await {
                tracing::error!("RoundManager encountered an error after setting RBCProcessor: {:?}", e);
            }
        });
    }

    /// **✅ Get `rbc_processor`, Ensuring It Exists**
    pub async fn get_rbc_processor(&self) -> Arc<RBCProcessor> {
        match &self.rbc_processor {
            Some(processor) => processor.clone(),
            None => panic!("`rbc_processor` has not been set! Call `set_rbc_processor()` first."),
        }
    }

    /// **Compute Fault Tolerance Threshold (f)**
    pub fn get_fault_tolerance_threshold(&self) -> usize {
        (self.total_nodes - 1) / 3
    }

    /// **Compute Quorum Threshold**
    pub fn get_quorum_threshold(&self) -> usize {
        let f = self.get_fault_tolerance_threshold();
        2 * f + 1
    }

    /// **Check if Quorum is Reached**
    pub async fn is_quorum_reached(&self, round_id: u64) -> bool {
        let quorum_threshold = self.get_quorum_threshold();
        let epoch_key = round_id.to_be_bytes().to_vec();

        let vote_count = {
            let quorum_votes = self.quorum_votes.lock().await;
            quorum_votes
                .get(&epoch_key)
                .map(|voters| voters.len())
                .unwrap_or(0)
        };

        // info!(
        //     "Node {}: Checking quorum for round {}. Unique votes: {}, Threshold: {}",
        //     self.id, round_id, vote_count, quorum_threshold
        // );

        vote_count >= quorum_threshold
    }


    pub async fn update_proposal_tracker(
        node: Arc<Mutex<Node>>,
        propose_request: ProposeRequest,
    ) -> Result<(usize, usize, Vec<ProposeRequest>), String> {
        let node_guard = node.lock().await;
        let node_id = node_guard.id;
        let round_id = propose_request.base.round_id;
    
        let mut proposal_tracker = node_guard.proposal_tracker.lock().await;
        let entry = proposal_tracker.entry(round_id).or_insert_with(HashMap::new);
        let proposal_count = entry.len();
        let quorum_threshold = node_guard.total_nodes - node_guard.get_fault_tolerance_threshold();
    
        // ❗ Critical freeze: once quorum reached, ignore late proposals
        if proposal_count >= quorum_threshold {
            tracing::info!(
                "Node {}: Proposal tracker locked for round {}, ignoring new proposals.",
                node_id, round_id
            );
        } else {
            entry.insert(propose_request.base.proposing_node_id as usize, propose_request.clone());
        }
    
        let updated_count = entry.len();
        let stored_proposals = entry.values().cloned().collect();
    
        tracing::info!(
            "Node {}: Added proposal for round {}. Count: {}/{}",
            node_id, round_id, updated_count, quorum_threshold
        );
    
        Ok((updated_count, quorum_threshold, stored_proposals))
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
        // tracing::info!("Node {}: Checking for parent unit: '{}'. Current DAG: {:?}", self.id, parent_id, *dag);
    
        let result = dag.values()
            .flatten()
            .any(|unit| unit.unit_id == parent_id);
    
        info!(
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
