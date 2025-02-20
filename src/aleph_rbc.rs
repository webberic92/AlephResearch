use aleph_research::controllers::api_routes::initialize_apis;
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::create_transaction_data::create_transaction_data;
use aleph_research::utils::start_util::wait_for_all_nodes_health;
use reqwest::Client;
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;
use aleph_research::utils::config_util::load_config;
use aleph_research::structs::node::Node;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    
    let config = load_config(None);
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    let client = Arc::new(Client::new());
    
    // ✅ Create Node with Proposal Queue Handling
    let node = Node::new(
        config.node.id,
        config.network.total_nodes,
        config.network.ip_address.clone(),
        config.network.nodes.clone(),
        config.network.ip_manager_address.clone(),
        config.consensus.number_of_transactions.clone(),
        config.consensus.transaction_size.clone(),
        config.consensus.data_shards.clone(),
        config.consensus.total_rounds.clone(),
        client.clone(),
    );
    
    // ✅ Node is already Arc<Mutex<Node>>, so no need to call `.read()`
    let node_clone = Arc::clone(&node);
    let client_clone = Arc::clone(&client);
    
    let app = initialize_apis(node.clone(), client.clone());
    wait_for_all_nodes_health(&client, node.clone()).await?;
    
    // ✅ **Spawn the Transaction Execution Logic in Tokio (Non-Blocking)**
    tokio::spawn(async move {
        if let Err(e) = execute_transaction_logic(node_clone, client_clone).await {
            error!("Transaction execution failed: {:?}", e);
        }
    });
    
    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn execute_transaction_logic(
    node: Arc<Mutex<Node>>, 
    client: Arc<Client>,
) -> Result<(), anyhow::Error> {  

    // ✅ Wait for all nodes to become healthy before starting.

    // 🎯 Create transaction data
    match create_transaction_data(node.clone()).await {
        Ok((shards, merkle_roots, parents)) => {
            let node_id = {
                let node_guard = node.lock().await;
                node_guard.id
            };
            let round = 1; // Assuming round 1 for single-round logic

            info!("Node: Transaction data created for round {}.", round);

            // 🎯 Step 2: Send proposals for this round
            info!("Node {}: Sending proposals for round {}...", node_id, round);
            if let Err(e) = send_proposals(client, node.clone(), &shards, &merkle_roots, parents).await {
                error!(
                    "Node {}: Failed to send proposals for round {}. Error: {:?}",
                    node_id, round, e
                );
            } else {
                info!(
                    "Node {}: Proposals for round {} sent successfully.",
                    node_id, round
                );
            }
        }
        Err(e) => {
            let node_id = {
                let node_guard = node.lock().await;
                node_guard.id
            };
            let round = 1;
            error!(
                "Node {}: Failed to create transaction data for round {}. Error: {:?}",
                node_id, round, e
            );
        }
    }

    info!("Node: All rounds completed successfully.");
    Ok(())
}














