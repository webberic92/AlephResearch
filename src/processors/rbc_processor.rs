use chrono::round;
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
use super::priority_queue::RBCMessage;

/// Processor with a **priority queue**
pub struct RBCProcessor {
    queue_tx: mpsc::Sender<RBCMessage>,
}

impl RBCProcessor {
    pub fn new(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCMessage>(100); // ✅ FIFO Queue

        let node_clone = node.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            let mut priority_queue = BinaryHeap::new();
            let mut fifo_queues: [VecDeque<RBCMessage>; 3] = [VecDeque::new(), VecDeque::new(), VecDeque::new()];

            while let Some(msg) = rx.recv().await {
                priority_queue.push(msg); // **Push message to priority queue**

                // ✅ Take the highest-priority message and insert it into the correct FIFO queue
                while let Some(task) = priority_queue.pop() {
                    match task.priority() {
                        1 => fifo_queues[0].push_back(task), // ✅ Commit Queue (Highest Priority)
                        2 => fifo_queues[1].push_back(task), // ✅ Prevote Queue
                        3 => fifo_queues[2].push_back(task), // ✅ Proposal Queue
                        _ => continue, // ✅ Prevent message loss
                    }
                }

                // ✅ Process FIFO queues
                let node_ref = node_clone.clone();
                let client_ref = client_clone.clone();

                loop {
                    let mut processed_any = false;

                    // ✅ Always prioritize commits first
                    for (i, queue) in fifo_queues.iter_mut().enumerate() {
                        if let Some(task) = queue.pop_front() {
                            processed_any = true; // ✅ Track processed task

                            match task {
                                RBCMessage::Commit(commit) => {
                                    info!("Entering: Processing commit for round {}", commit.round_id);
                                    let node = node_ref.clone();
                                    let client = client_ref.clone();
                                    let round_id = commit.round_id;
                                    if let Err(e) = process_commit(node, client, commit).await {
                                        error!("Error processing commit: {:?}", e);
                                    }
                                    info!("Leaving: Processing commit for round {}", round_id);

                                }
                                RBCMessage::Prevote(prevote) => {
                                    info!("Entering: Processing prevote for round {}", prevote.proposals[0].base.round_id);
                                    let node = node_ref.clone();
                                    let client = client_ref.clone();
                                    let round_id = prevote.proposals[0].base.round_id;
                                    if let Err(e) = process_prevote(node, client, prevote).await {
                                        error!("Error processing prevote: {:?}", e);
                                    }
                                    info!("Leaving: Processing prevote for round {}",round_id);

                                }
                                RBCMessage::Proposal(propose) => {
                                    info!("Entering: Processing proposal for round {}", propose.base.round_id);
                                    let node = node_ref.clone();
                                    let client = client_ref.clone();
                                    let round_id = propose.base.round_id;
                                    if let Err(e) = process_proposal(node, client, propose).await {
                                        error!("Error processing proposal: {:?}", e);
                                    }
                                    info!("Leaving: Processing proposal for round {}",round_id);

                                }
                            }
                        }
                    }

                    // ✅ If no tasks were processed, exit the loop
                    if !processed_any {
                        break;
                    }
                }
            }
        });

        Self { queue_tx: tx }
    }

    /// ✅ Enqueue messages into the queue instead of processing them directly
    pub async fn enqueue_message(&self, msg: RBCMessage) {
        if let Err(e) = self.queue_tx.send(msg).await {
            error!("Failed to enqueue message: {:?}", e);
        }
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
