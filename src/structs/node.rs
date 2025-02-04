use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use reqwest::Client;
use tokio::sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock};
use tracing::{error, info};
use crate::handlers::handle_propose::handle_propose;
use super::requests::ProposeRequest;

/// **📌 Node Struct: Represents a single node in the Aleph RBC protocol.**
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub total_nodes: usize,
    pub quorum_votes: Arc<RwLock<HashMap<Vec<u8>, usize>>>,
    pub current_epoch: Arc<Mutex<u64>>, // Tracks the current epoch explicitly
    pub proposal_tracker: Arc<Mutex<HashMap<usize, ProposeRequest>>>, // Stores proposals instead of just IDs
    pub finalized_blocks: Arc<Mutex<HashSet<Vec<u8>>>>,
    pub dag: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
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
            finalized_blocks: Arc::new(Mutex::new(HashSet::new())),
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

    /// **🔹 Store Finalized Block**
    pub async fn output_finalized_block(&self, block: Vec<u8>) {
        let mut finalized_blocks = self.finalized_blocks.lock().await;
        if finalized_blocks.insert(block.clone()) {
            info!("Node {}: Finalized block: {:?}", self.id, block);
        } else {
            info!("Node {}: Block {:?} is already finalized.", self.id, block);
        }
    }

    /// **🔹 Check if Units Are Committed**
    pub async fn is_unit_committed(&self, unit_ids: &[Vec<u8>]) -> bool {
        let finalized_blocks = self.finalized_blocks.lock().await;
        unit_ids.iter().all(|unit_id| finalized_blocks.contains(unit_id))
    }

    /// **🔹 Update Proposal Tracker**
    /// - Stores received proposals and tracks count.
    pub async fn update_proposal_tracker(
        node: Arc<RwLock<Node>>,
        propose_request: ProposeRequest,
    ) -> Result<(usize, usize, Vec<ProposeRequest>), String> {
        let node_id = node.read().await.id;

        info!("Node {}: Acquiring write lock for proposal tracker update...", node_id);

        let proposal_count;
        let required_proposals;
        let stored_proposals;

        {
            let node_write = node.write().await;
            let mut proposal_tracker = node_write.proposal_tracker.lock().await;

            // Store proposal in the tracker
            proposal_tracker.insert(propose_request.base.proposing_node_id, propose_request.clone());

            proposal_count = proposal_tracker.len();
            stored_proposals = proposal_tracker.values().cloned().collect();
        }

        {
            let node_read = node.read().await;
            let node_count = node_read.total_nodes;
            let f = node_read.get_fault_tolerance_threshold();
            required_proposals = node_count - f;
        }

        info!(
            "Node {}: Proposal added from Node {}. Total proposals: {}. Required for consensus: {}.",
            node_id, propose_request.base.proposing_node_id, proposal_count, required_proposals
        );

        Ok((proposal_count, required_proposals, stored_proposals))
    }
}
