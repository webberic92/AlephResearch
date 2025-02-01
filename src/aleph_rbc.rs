use aleph_research::controllers::api_routes::initialize_apis;
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::create_transaction_data::create_transaction_data;
use aleph_research::utils::start_util::{wait_for_all_nodes_health, wait_for_turn};
use reqwest::Client;
use tokio::sync::RwLock;
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;
use aleph_research::utils::config_util::load_config;
use aleph_research::structs;
use structs::node::Node;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = load_config(None);
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    let client = Arc::new(Client::new());

    // Wrap Node inside Arc<RwLock<Node>>
    let node = Arc::new(tokio::sync::RwLock::new(Node::new(
        config.node.id,
        config.node.total_nodes,
        config.network.ip_address,
        config.network.nodes,
        config.network.ip_manager_address,
    )));

    let node_clone = node.clone(); // Clone for use in transaction logic

    let app = initialize_apis(node.clone(), client.clone());

    // Spawn the transaction logic in a separate task
    tokio::spawn(async move {
        if let Err(e) = execute_transaction_logic(node_clone, client.clone()).await {
            error!("Transaction execution failed: {}", e);
        }
    });

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}


async fn execute_transaction_logic(node: Arc<RwLock<Node>>, client: Arc<Client>) -> Result<(), Box<dyn std::error::Error>> {
    // Wait for all nodes to be healthy
    wait_for_all_nodes_health(&client, node.clone()).await;
    wait_for_turn(&client, node.clone()).await?;

    // Generate transaction data
    let (shards, merkle_root) = create_transaction_data().await?;

    // Send proposals
    send_proposals(&client, node.clone(), &shards, &merkle_root).await?;

    // Update in-memory node state instead of TOML
    {
        let node_write = node.write().await;
        let mut epoch = node_write.current_epoch.lock().await; // Lock the mutex before modifying
        *epoch += 1; // Increment epoch after proposal
        info!("Updated in-memory epoch to {}", *epoch);
    }

    Ok(())
}
