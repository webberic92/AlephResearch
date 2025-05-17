use chrono::Local;
use tokio::sync::{Mutex, mpsc};
use tracing::{info, warn, error};
use std::sync::Arc;
use std::collections::{BinaryHeap, VecDeque};
use reqwest::Client;

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
        let (tx, mut rx) = mpsc::channel::<RBCMessage>(100);
        let tx_clone = tx.clone();
        let node_clone = node.clone();

        tokio::spawn(async move {
            let mut priority_queue = BinaryHeap::new();
            let mut fifo_queues: Vec<VecDeque<RBCMessage>> = vec![VecDeque::new(), VecDeque::new(), VecDeque::new()];
            let mut last_round = 0;

            while let Some(msg) = rx.recv().await {
                match &msg {
                    RBCMessage::RoundFinalized(new_round) => {
                        last_round = *new_round;
                        Self::handle_round_finalized(
                            *new_round,
                            &node_clone,
                            &tx,
                            &mut priority_queue,
                            &mut fifo_queues
                        ).await;
                        continue;
                    }
                    RBCMessage::Commit(req) if req.round_id < last_round => continue,
                    RBCMessage::Prevote(req) if req.proposals[0].base.round_id < last_round => continue,
                    RBCMessage::Proposal(req) if req.base.round_id < last_round => continue,
                    _ => {}
                }

                priority_queue.push(msg);
                Self::drain_priority_queue(&mut priority_queue, &node_clone).await;
            }

            info!("✅ RBCProcessor: Shutting down gracefully.");
        });

        Self { queue_tx: tx_clone }
    }

    /// ✅ Public method to enqueue new messages
    pub async fn enqueue_message(&self, msg: RBCMessage) {
        let result = self.queue_tx.send(msg).await;
        if let Err(e) = result {
            error!("Failed to enqueue message: {:?}", e);
        } 
    }

    /// ✅ Process all messages from the priority queue
    async fn drain_priority_queue(
        queue: &mut BinaryHeap<RBCMessage>,
        node: &Arc<Mutex<Node>>,
    ) {
        while let Some(task) = queue.pop() {
            match task {
                RBCMessage::Commit(commit) => {
                    info!("Processing commit for round {} from node {}", commit.round_id, commit.proposing_node_id);
                    if let Err(e) = process_commit(node.clone(), commit).await {
                        error!("Error processing commit: {:?}", e);
                    }
                }
                RBCMessage::Prevote(prevote) => {
                    info!("Processing prevote for round {} from node {}", prevote.proposals[0].base.round_id, prevote.sender_url);
                    if let Err(e) = process_prevote(node.clone(), prevote).await {
                        error!("Error processing prevote: {:?}", e);
                    }
                }
                RBCMessage::Proposal(propose) => {
                    info!("Processing proposal for round {} from node {}", propose.base.round_id, propose.base.proposing_node_id);
                    if let Err(e) = process_proposal(node.clone(), propose).await {
                        error!("Error processing proposal: {:?}", e);
                    }
                }
                _ => {}
            }
        }
    }

    /// ✅ Handle round finalization and create the next proposal
    async fn handle_round_finalized(
        new_round: u64,
        node: &Arc<Mutex<Node>>,
        tx: &mpsc::Sender<RBCMessage>,
        priority_queue: &mut BinaryHeap<RBCMessage>,
        fifo_queues: &mut [VecDeque<RBCMessage>],
    ) {
        info!("🔄 RBCProcessor: RoundFinalized received for round {}", new_round);

        let total_rounds = node.lock().await.total_rounds;
        if new_round >= total_rounds.try_into().unwrap() {
            info!("✅ All {} rounds completed. Stopping RBCProcessor.", total_rounds);
            info!("📊 Ending Latency Logger: {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
            return;
        }

        let (commit_count, prevote_count, proposal_count) =
            Self::remove_stale_messages(new_round, priority_queue, fifo_queues);

        info!(
            "🧹 RBCProcessor: Removed stale messages → commits: {}, prevotes: {}, proposals: {}",
            commit_count, prevote_count, proposal_count
        );

        match create_transaction_data(node.clone()).await {
            Ok(propose_request) => {
                let round_id = propose_request.base.round_id;
                if send_proposals(node.clone(), propose_request.clone()).await.is_ok() {
                    info!("✅ Proposal for round {} sent successfully.", round_id);
                    if let Err(e) = tx.send(RBCMessage::Proposal(propose_request)).await {
                        error!("❌ Failed to enqueue proposal for round {}: {:?}", round_id, e);
                    }
                }
            }
            Err(e) => error!("❌ Failed to create transaction data for round {}: {:?}", new_round + 1, e),
        }
    }

    /// ✅ Clean up stale messages from queues for old rounds
    fn remove_stale_messages(
        current_round: u64,
        priority_queue: &mut BinaryHeap<RBCMessage>,
        fifo_queues: &mut [VecDeque<RBCMessage>],
    ) -> (usize, usize, usize) {
        let mut commit_count = 0;
        let mut prevote_count = 0;
        let mut proposal_count = 0;

        for queue in fifo_queues.iter_mut() {
            queue.retain(|msg| match msg {
                RBCMessage::Commit(req) if req.round_id < current_round => {
                    commit_count += 1;
                    false
                }
                RBCMessage::Prevote(req) if req.proposals[0].base.round_id < current_round => {
                    prevote_count += 1;
                    false
                }
                RBCMessage::Proposal(req) if req.base.round_id < current_round => {
                    proposal_count += 1;
                    false
                }
                _ => true,
            });
        }

        priority_queue.retain(|msg| match msg {
            RBCMessage::Commit(req) if req.round_id < current_round => {
                commit_count += 1;
                false
            }
            RBCMessage::Prevote(req) if req.proposals[0].base.round_id < current_round => {
                prevote_count += 1;
                false
            }
            RBCMessage::Proposal(req) if req.base.round_id < current_round => {
                proposal_count += 1;
                false
            }
            _ => true,
        });

        (commit_count, prevote_count, proposal_count)
    }
}

// ✅ Wrapper functions to call handlers
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
