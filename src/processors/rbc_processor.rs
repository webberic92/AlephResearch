use chrono::Local;
use tokio::sync::{Mutex, mpsc};
use tracing::{info, warn, error};
use std::sync::Arc;
use std::collections::BinaryHeap;
use tokio::task::yield_now;

use crate::handlers::handle_commit::handle_commit;
use crate::handlers::handle_prevote::handle_prevote;
use crate::handlers::handle_propose::handle_propose;
use crate::structs::{node::Node, requests::{CommitRequest, PrevoteRequest, ProposeRequest}};
use crate::utils::create_transaction_data::create_transaction_data;
use crate::requests::send_proposals::send_proposals;
use super::priority_queue::RBCMessage;

pub struct RBCProcessor {
    queue_tx: mpsc::Sender<RBCMessage>,
}

impl RBCProcessor {
    pub fn new(node: Arc<Mutex<Node>>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCMessage>(1000);
        let tx_clone = tx.clone();
        let node_clone = node.clone();
        let shared_queue = Arc::new(Mutex::new(BinaryHeap::<RBCMessage>::new()));
        let worker_queue = shared_queue.clone();

        tokio::spawn(async move {
            loop {
                let task_opt = {
                    let mut queue = worker_queue.lock().await;
                    queue.pop()
                };

                if let Some(task) = task_opt {
                    task.log_enqueue();
                    match task {
                        RBCMessage::Commit(commit) => {
                            info!("Processing commit for round {} from node {}", commit.round_id, commit.proposing_node_id);
                            if let Err(e) = process_commit(node_clone.clone(), commit).await {
                                error!("Error processing commit: {:?}", e);
                            }
                        }
                        RBCMessage::Prevote(prevote) => {
                            if prevote.proposals.is_empty() {
                                error!("❌ Received prevote with empty proposals. Skipping.");
                                continue;
                            }
                            info!("Processing prevote for round {} from node {}", prevote.proposals[0].base.round_id, prevote.sender_url);
                            if let Err(e) = process_prevote(node_clone.clone(), prevote).await {
                                error!("Error processing prevote: {:?}", e);
                            }
                        }
                        RBCMessage::Proposal(propose) => {
                            info!("Processing proposal for round {} from node {}", propose.base.round_id, propose.base.proposing_node_id);
                            if let Err(e) = process_proposal(node_clone.clone(), propose).await {
                                error!("Error processing proposal: {:?}", e);
                            }
                        }
                        _ => {}
                    }
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                yield_now().await;
            }
        });

        let queue_for_rx = shared_queue.clone();
        let node_for_rx = node.clone();
        tokio::spawn(async move {
            let mut last_round = 0;
            while let Some(msg) = rx.recv().await {
                let should_skip = match &msg {
                    RBCMessage::RoundFinalized(new_round) => {
                        last_round = *new_round;
                        Self::sanitize_stale_messages(*new_round, &queue_for_rx).await;
                        Self::handle_round_finalized(*new_round, &node_for_rx, &tx).await;
                        true
                    }
                    RBCMessage::Commit(req) => req.round_id < last_round,
                    RBCMessage::Prevote(req) => req.proposals.get(0).map_or(true, |p| p.base.round_id < last_round),
                    RBCMessage::Proposal(req) => req.base.round_id < last_round,
                    // No action needed for unhandled message types
                };

                if !should_skip {
                    queue_for_rx.lock().await.push(msg);
                }
            }
            info!("✅ RBCProcessor: Shutting down gracefully.");
        });

        Self { queue_tx: tx_clone }
    }

    pub async fn enqueue_message(&self, msg: RBCMessage) {
        if let Err(e) = self.queue_tx.send(msg).await {
            error!("Failed to enqueue message: {:?}", e);
        }
    }

    async fn sanitize_stale_messages(round: u64, queue: &Arc<Mutex<BinaryHeap<RBCMessage>>>) {
        let mut pq = queue.lock().await;
    
        // Count messages before sanitizing
        let mut before_round_finalized = 0;
        let mut before_commits = 0;
        let mut before_prevotes = 0;
        let mut before_proposals = 0;
    
        for msg in pq.iter() {
            match msg {
                RBCMessage::RoundFinalized(_) => before_round_finalized += 1,
                RBCMessage::Commit(_) => before_commits += 1,
                RBCMessage::Prevote(_) => before_prevotes += 1,
                RBCMessage::Proposal(_) => before_proposals += 1,
            }
        }
    
        info!(
            "🧹 Sanitizing stale messages for round {}. Queue size = {} → RoundFinalized: {}, Commits: {}, Prevotes: {}, Proposals: {}",
            round,
            pq.len(),
            before_round_finalized,
            before_commits,
            before_prevotes,
            before_proposals
        );
    
        // Sanitize
        pq.retain(|msg| match msg {
            RBCMessage::Commit(req) => req.round_id >= round,
            RBCMessage::Prevote(req) => req.proposals.get(0).map_or(false, |p| p.base.round_id >= round),
            RBCMessage::Proposal(req) => req.base.round_id >= round,
            _ => true,
        });
    
        // Count messages after sanitizing
        let mut after_round_finalized = 0;
        let mut after_commits = 0;
        let mut after_prevotes = 0;
        let mut after_proposals = 0;
    
        for msg in pq.iter() {
            match msg {
                RBCMessage::RoundFinalized(_) => after_round_finalized += 1,
                RBCMessage::Commit(_) => after_commits += 1,
                RBCMessage::Prevote(_) => after_prevotes += 1,
                RBCMessage::Proposal(_) => after_proposals += 1,
            }
        }
    
        info!(
            "🧹 Sanitized complete. Remaining — RoundFinalized: {}, Commits: {}, Prevotes: {}, Proposals: {} (Total: {})",
            after_round_finalized,
            after_commits,
            after_prevotes,
            after_proposals,
            pq.len()
        );
    }
    

    async fn handle_round_finalized(
        new_round: u64,
        node: &Arc<Mutex<Node>>,
        tx: &mpsc::Sender<RBCMessage>,
    ) {
        info!("🔄 RBCProcessor: RoundFinalized received for round {}", new_round);

        let total_rounds = node.lock().await.total_rounds;
        if new_round >= total_rounds.try_into().unwrap() {
            info!("✅ All {} rounds completed. Stopping RBCProcessor.", total_rounds);
            info!("📊 Ending Latency Logger: {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
            return;
        }

        match create_transaction_data(node.clone()).await {
            Ok(propose_request) => {
                let round_id = propose_request.base.round_id;
                if send_proposals(node.clone(), propose_request.clone()).await.is_ok() {
                    info!("✅ Proposal for round {} sent successfully.", round_id);
                    if let Err(e) = tx.try_send(RBCMessage::Proposal(propose_request.clone())) {
                        warn!("⚠️ Queue full for proposal round {}. Dropping: {:?}", round_id, e);
                    }
                }
            }
            Err(e) => error!("❌ Failed to create transaction data for round {}: {:?}", new_round + 1, e),
        }
    }
}

async fn process_proposal(node: Arc<Mutex<Node>>, propose_request: ProposeRequest) -> Result<(), String> {
    if !should_process_request(node.clone(), propose_request.base.round_id, propose_request.base.proposing_node_id as usize, "Propose".into()).await {
        return Ok(());
    }
    handle_propose(node, propose_request).await
}

async fn process_prevote(node: Arc<Mutex<Node>>, prevote_request: PrevoteRequest) -> Result<(), String> {
    if !should_process_request(node.clone(), prevote_request.proposals[0].base.round_id, prevote_request.proposals[0].base.proposing_node_id as usize, "Prevote".into()).await {
        return Ok(());
    }
    handle_prevote(node, prevote_request).await
}

async fn process_commit(node: Arc<Mutex<Node>>, commit_request: CommitRequest) -> Result<(), String> {
    if !should_process_request(node.clone(), commit_request.round_id, commit_request.proposing_node_id as usize, "Commit".into()).await {
        return Ok(());
    }
    handle_commit(node, commit_request).await
}

async fn should_process_request(node: Arc<Mutex<Node>>, request_round: u64, proposer_id: usize, req_type: String) -> bool {
    let node_guard = node.lock().await;
    let dag_guard = node_guard.dag.lock().await;
    if dag_guard.contains_key(&request_round) {
        warn!(
            "Node {}: Round {} is finalized. Ignoring {} request from node {}.",
            node_guard.id, request_round, req_type, proposer_id
        );
        return false;
    }
    true
}
