use reqwest::Client;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error};
use tokio::sync::Mutex;
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{node::Node, requests::CommitRequest},
};

#[derive(Debug)]
enum RBCCommitMessage {
    Commit(CommitRequest),
}

pub struct RBCProcessorCommit {
    queue_tx: mpsc::Sender<RBCCommitMessage>,
}

impl RBCProcessorCommit {
    pub fn new(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCCommitMessage>(100); // FIFO Queue

        let node_clone = node.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    RBCCommitMessage::Commit(commit) => {
                        let node = node_clone.clone();
                        let client = client_clone.clone();

                        if let Err(e) = process_commit(node, client, commit).await {
                            error!("Error processing commit: {:?}", e);
                        }
                    }
                }
            }
        });

        Self { queue_tx: tx }
    }

    pub async fn enqueue_commit(&self, msg: CommitRequest) {
        if let Err(e) = self.queue_tx.send(RBCCommitMessage::Commit(msg)).await {
            error!("Failed to enqueue commit: {:?}", e);
        }
    }
}

// ✅ Calls `handle_commit` with proper parameters
async fn process_commit(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    commit_request: CommitRequest
) -> Result<(), String> {
    handle_commit(node, client,commit_request).await
}
