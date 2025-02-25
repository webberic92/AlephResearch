use reqwest::Client;
use tokio::sync::mpsc;
use std::sync::Arc;
use tracing::{info, error};
use tokio::sync::Mutex;
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{node::Node, requests::PrevoteRequest},
};

#[derive(Debug)]
enum RBCPrevoteMessage {
    Prevote(PrevoteRequest),
}

pub struct RBCProcessorPrevote {
    queue_tx: mpsc::Sender<RBCPrevoteMessage>,
}

impl RBCProcessorPrevote {
    pub fn new(node: Arc<Mutex<Node>>, client: Arc<Client>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RBCPrevoteMessage>(100); // FIFO Queue

        let node_clone = node.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    RBCPrevoteMessage::Prevote(prevote) => {
                        let node = node_clone.clone();
                        let client = client_clone.clone();

                        if let Err(e) = process_prevote(node, client, prevote).await {
                            error!("Error processing prevote: {:?}", e);
                        }
                    }
                }
            }
        });

        Self { queue_tx: tx }
    }

    pub async fn enqueue_prevote(&self, msg: PrevoteRequest) {
        if let Err(e) = self.queue_tx.send(RBCPrevoteMessage::Prevote(msg)).await {
            error!("Failed to enqueue prevote: {:?}", e);
        }
    }
}

async fn process_prevote(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>, 
    prevote_request: PrevoteRequest
) -> Result<(), String> {
    handle_prevote(node, client, prevote_request).await
}
