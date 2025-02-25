use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use tracing::{error, info};
use crate::{structs::node::Node, requests::send_proposals::send_proposals, utils::create_transaction_data::create_transaction_data};
use crate::utils::events::Event;

pub async fn round_manager_task(
    node: Arc<Mutex<Node>>, 
    mut event_receiver: mpsc::Receiver<Event>,
) -> Result<(), anyhow::Error> {
    while let Some(event) = event_receiver.recv().await {
        match event {
            Event::RoundFinalized(r) => {
                info!("Round Manager: Processing RoundFinalized event for round {}", r);

                {
                    let node_guard = node.lock().await;
                    let total_rounds = node_guard.total_rounds;

                    if r >= total_rounds.try_into().unwrap() {
                        info!("Node {}: All rounds completed. Stopping Round Manager.", node_guard.id);
                        break;
                    }
                } // 🔓 Release the lock before sending a proposal

                if let Err(e) = create_and_propose_round(node.clone(), r + 1).await {
                    error!("Failed to start next round {}: {:?}", r + 1, e);
                }
            }
        }
    }
    Ok(())
}

async fn create_and_propose_round(
    node: Arc<Mutex<Node>>, 
    r: u64, 
) -> Result<(), anyhow::Error> {
    let propose_request = create_transaction_data(node.clone()).await?;
    let node_guard = node.lock().await;

    info!(
        "Node {}: Created and sending proposal for round {}.",
        node_guard.id, r
    );

    if let Err(e) = send_proposals(node_guard.client.clone(), node.clone(), propose_request).await {
        error!("Node {}: Failed to send proposal for round {}. Error: {:?}", node_guard.id, r, e);
    } else {
        info!("Node {}: Successfully sent proposal for round {}.", node_guard.id, r);
    }

    Ok(())
}
