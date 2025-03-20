use chrono::Local;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{info, error};
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

/// **Processor with a priority queue**
pub struct RBCProcessor {
    queue_tx: mpsc::Sender<RBCMessage>,
}

impl RBCProcessor {
    pub fn new(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCMessage>(100);
        let tx_clone = tx.clone();
        let node_clone = node.clone();
        let client_clone = client.clone();
        let mut last_round = 0;

        tokio::spawn(async move {
            let mut priority_queue = BinaryHeap::new();
            let mut fifo_queues: Vec<VecDeque<RBCMessage>> = vec![VecDeque::new(), VecDeque::new(), VecDeque::new()];

            while let Some(msg) = rx.recv().await {
                match &msg {
                    RBCMessage::RoundFinalized(new_round) => {
                        info!("🔄 RBCProcessor: RoundFinalized received for round {}", new_round);
                        last_round = *new_round;

                        // ✅ Acquire node lock before checking total rounds
                        //info!("🔍 [DEBUG] RBCProcessor waiting to acquire node lock before checking termination condition.");
                        let node_guard = node_clone.lock().await;
                        //info!("🔓 [DEBUG] RBCProcessor acquired node lock for termination check.");

                        let total_rounds = node_guard.total_rounds;
                        if last_round >= total_rounds.try_into().unwrap() {
                            info!("✅ RBCProcessor: All {} rounds completed. Stopping RBCProcessor.", total_rounds);
                            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                            info!("Ending Latency Logger : TIME {}", current_time);
                            break; // 🔥 **Exit the loop when total rounds are reached**
                        }

                        let mut commit_count = 0;
                        let mut prevote_count = 0;
                        let mut proposal_count = 0;

                        // ✅ Remove stale messages from FIFO queues
                        for queue in fifo_queues.iter_mut() {
                            queue.retain(|msg| match msg {
                                RBCMessage::Commit(req) if req.round_id < last_round => {
                                    commit_count += 1;
                                    false
                                }
                                RBCMessage::Prevote(req) if req.proposals[0].base.round_id < last_round => {
                                    prevote_count += 1;
                                    false
                                }
                                RBCMessage::Proposal(req) if req.base.round_id < last_round => {
                                    proposal_count += 1;
                                    false
                                }
                                _ => true,
                            });
                        }

                        // ✅ Remove stale messages from priority queue
                        priority_queue.retain(|msg| match msg {
                            RBCMessage::Commit(req) if req.round_id < last_round => {
                                commit_count += 1;
                                false
                            }
                            RBCMessage::Prevote(req) if req.proposals[0].base.round_id < last_round => {
                                prevote_count += 1;
                                false
                            }
                            RBCMessage::Proposal(req) if req.base.round_id < last_round => {
                                proposal_count += 1;
                                false
                            }
                            _ => true,
                        });

                        info!(
                            "🧹 RBCProcessor: Removed stale messages. Deleted: {} commits, {} prevotes, {} proposals.",
                            commit_count, prevote_count, proposal_count
                        );

                        drop(node_guard); // ✅ Release lock before creating the next proposal

                        info!("🔄 RBCProcessor: Creating and proposing next round {}", new_round + 1);

                        // ✅ Step 1: **Create Transaction Data**
                        match create_transaction_data(node_clone.clone()).await {
                            Ok(propose_request) => {
                                let round_id = propose_request.base.round_id;
                                info!("✅ RBCProcessor: Successfully created transaction data for round {}.", round_id);

                                // ✅ Step 2: **Send the proposal**
                                if let Err(e) = send_proposals(client_clone.clone(), node_clone.clone(), propose_request.clone()).await {
                                    error!("❌ Failed to send proposal for round {}: {:?}", round_id, e);
                                } else {
                                    info!("✅ RBCProcessor: Proposal for round {} sent successfully.", round_id);
                                }

                                // ✅ Step 3: **Enqueue the proposal for processing**
                                let proposal_message = RBCMessage::Proposal(propose_request);
                                if let Err(e) = tx.send(proposal_message).await {
                                    error!("❌ Failed to enqueue proposal for round {}: {:?}", round_id, e);
                                } else {
                                    info!("📥 RBCProcessor: Proposal for round {} added to queue.", round_id);
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to create transaction data for round {}: {:?}", new_round + 1, e);
                            }
                        }
                        continue;
                    }
                    RBCMessage::Commit(req) if req.round_id < last_round => continue,
                    RBCMessage::Prevote(req) if req.proposals[0].base.round_id < last_round => continue,
                    RBCMessage::Proposal(req) if req.base.round_id < last_round => continue,
                    _ => {}
                }

                priority_queue.push(msg);

                while let Some(task) = priority_queue.pop() {
                    match task {
                        RBCMessage::Commit(commit) => {
                            let node = node_clone.clone();
                            let client = client_clone.clone();
                            info!("Processing commit for round {}", commit.round_id);
                            if let Err(e) = process_commit(node, client, commit).await {
                                error!("Error processing commit: {:?}", e);
                            }
                        }
                        RBCMessage::Prevote(prevote) => {
                            let node = node_clone.clone();
                            let client = client_clone.clone();
                            info!("Processing prevote for round {}", prevote.proposals[0].base.round_id);
                            if let Err(e) = process_prevote(node, client, prevote).await {
                                error!("Error processing prevote: {:?}", e);
                            }
                        }
                        RBCMessage::Proposal(propose) => {
                            let node = node_clone.clone();
                            let client = client_clone.clone();
                            info!("Processing proposal for round {}", propose.base.round_id);
                            if let Err(e) = process_proposal(node, client, propose).await {
                                error!("Error processing proposal: {:?}", e);
                            }
                        }
                        _ => {}
                    }
                }
            }
            info!("✅ RBCProcessor: Shutting down gracefully.");
        });
        Self { queue_tx: tx_clone }
    }

    /// ✅ Enqueue messages into the queue instead of processing them directly
    pub async fn enqueue_message(&self, msg: RBCMessage) {
        if let Err(e) = self.queue_tx.send(msg).await {
            error!("Failed to enqueue message: {:?}", e);
        }
    }

    pub fn clear_queues(&self) {
        info!("🧹 RBCProcessor: Clearing internal queues...");

        // No need for locks, as this is only called when the processor is dropped
        let mut priority_queue: BinaryHeap<RBCMessage> = BinaryHeap::new();
        let mut fifo_queues: Vec<VecDeque<RBCMessage>> = vec![VecDeque::new(), VecDeque::new(), VecDeque::new()];

        priority_queue.clear();
        for queue in fifo_queues.iter_mut() {
            queue.clear();
        }

        info!("✅ RBCProcessor: Queues cleared successfully.");
    }



}




// ✅ Calls `handle_*` functions with `rbc_processor`
async fn process_proposal(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    propose_request: ProposeRequest
) -> Result<(), String> {
    handle_propose(node, client, propose_request).await
}

async fn process_prevote(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    prevote_request: PrevoteRequest
) -> Result<(), String> {
    handle_prevote(node, client, prevote_request).await
}

async fn process_commit(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    commit_request: CommitRequest
) -> Result<(), String> {
    handle_commit(node, client, commit_request).await
}
