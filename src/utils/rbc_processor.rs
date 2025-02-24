use reqwest::Client;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error};
use tokio::sync::Mutex;
use crate::{
    handlers::handle_propose::handle_propose,
    structs::{node::Node, requests::ProposeRequest},
};

#[derive(Debug)]
enum RBCMessage {
    Proposal(ProposeRequest),
}

pub struct RBCProcessor {
    queue_tx: mpsc::Sender<RBCMessage>,
}

impl RBCProcessor {
    pub fn new(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCMessage>(100); // FIFO Queue

        // Worker that processes proposals **sequentially**
        let node_clone = node.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    RBCMessage::Proposal(propose) => {
                        let node = node_clone.clone();
                        let client = client_clone.clone();

                        if let Err(e) = process_proposal(node, client, propose).await {
                            error!("Error processing proposal: {:?}", e);
                        }
                    }
                }
            }
        });

        Self { queue_tx: tx }
    }

    pub async fn enqueue_proposal(&self, msg: ProposeRequest) {
        if let Err(e) = self.queue_tx.send(RBCMessage::Proposal(msg)).await {
            error!("Failed to enqueue proposal: {:?}", e);
        }
    }
}

// ✅ Calls `handle_propose` with proper parameters
async fn process_proposal(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    propose_request: ProposeRequest
) -> Result<(), String> {
    handle_propose(node, client, propose_request).await
}
