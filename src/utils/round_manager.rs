use reqwest::Client;
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
                info!("🔄 Round Manager: Processing RoundFinalized event for round {}", r);

                {
                    //info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", r);
                    let node_guard = node.lock().await;
                    //info!("🔓 [DEBUG] Acquired node lock for round {}", r);
                    let total_rounds = node_guard.total_rounds;

                    if r >= total_rounds.try_into().unwrap() {
                        info!("✅ Node {}: All rounds completed. Stopping Round Manager.", node_guard.id);
                        break;
                    }
                } // 🔓 Release the lock before creating the next proposal

                // ✅ **Execute the transaction logic for the next round**
                if let Err(e) = execute_transaction_logic(node.clone()).await {
                    error!("❌ Failed to start next round {}: {:?}", r + 1, e);
                }
            }
        }
    }
    Ok(())
}

async fn execute_transaction_logic(
    node: Arc<Mutex<Node>>, 
) -> Result<(), anyhow::Error> {  

    // ✅ Create transaction proposal with multiple transactions
    match create_transaction_data(node.clone()).await {
        Ok(propose_request) => {
            let node_id = {
                //info!("🔍 [DEBUG] Waiting to acquire node lock for round {}", propose_request.base.round_id);
                let node_guard = node.lock().await;
                //info!("🔓 [DEBUG] Acquired node lock for round {}", propose_request.base.round_id);
                node_guard.id
            };

            let round = propose_request.base.round_id;

            info!("📢 Node {}: Created proposal with {} transactions for round {}.", 
                node_id, propose_request.transactions.len(), round
            );

            // ✅ Step 2: Send proposal
            info!("📤 Node {}: Sending proposal for round {}...", node_id, round);
            if let Err(e) = send_proposals( node.clone(), propose_request).await {
                error!(
                    "❌ Node {}: Failed to send proposal for round {}. Error: {:?}",
                    node_id, round, e
                );
            } else {
                info!(
                    "✅ Node {}: Proposal for round {} sent successfully.",
                    node_id, round
                );
            }
        }
        Err(e) => {
            let node_id = {
                //info!("🔍 [DEBUG] Waiting to acquire node lock for aleph_rbc_rs");
                let node_guard = node.lock().await;
                //info!("🔓 [DEBUG] Acquired node lock for aleph_rbc_rs");
                node_guard.id
            };
            error!(
                "❌ Node {}: Failed to create transaction proposal. Error: {:?}",
                node_id, e
            );
        }
    }

    info!("✅ Node: All rounds completed successfully.");
    Ok(())
}
