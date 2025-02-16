use aleph_research::controllers::api_routes::initialize_apis;
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::create_transaction_data::create_transaction_data;
use aleph_research::utils::start_util::{wait_for_all_nodes_health, wait_for_turn};
use reqwest::Client;
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};
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
    );

    // ✅ Node is already Arc<Mutex<Node>>, so no need to call `.read()`
    let node_clone = Arc::clone(&node);
    let client_clone = Arc::clone(&client);

    let app = initialize_apis(node.clone(), client.clone());

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
    wait_for_all_nodes_health(&client, node.clone()).await?;
   
    // 🔍 Access `total_rounds` safely within an async block
    let total_rounds = {
        let node_guard = node.lock().await;
        node_guard.total_rounds
    };

    for round in 1..=total_rounds {
        info!("Node: Starting round {} of {}", round, total_rounds);

        send_proposals_with_sync(node.clone(), round, client.clone()).await;


        // // 🎯 Step 3: Wait for round commit confirmation
        info!("Node {}: starting wait_for_commit_confirmation for round {}.",  node.lock().await.id, round);
        wait_for_commit_confirmation(node.clone(), round).await?;
        info!("Node {}: Commit confirmed for round {}.",  node.lock().await.id, round);
    }

    info!("Node: All rounds completed successfully.");
    Ok(())
}


pub async fn send_proposals_with_sync(node: Arc<Mutex<Node>>, round: usize, client: Arc<Client>) {
    // For the first round, no DAG sync needed.
    if round > 1 {
        while node.lock().await.dag.lock().await.get(&(round as u64 - 1)).is_none() {
            info!(
                "Node {}: Waiting for DAG round {} to be committed before sending round {} proposals...",
                node.lock().await.id, round - 1, round
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    info!(
        "Node {}: DAG ready. Proceeding to round {} proposal dispatch.",
        node.lock().await.id, round
    );

    // 🎯 Step 1: Create transaction data for this round
    info!("Node: Creating transaction data for round {}...", round);
    match create_transaction_data(node.clone()).await {
        Ok((shards, merkle_root, parents)) => {
            info!("Node: Transaction data created for round {}.", round);

            // 🎯 Step 2: Send proposals for this round
            info!(
                "Node {}: Sending proposals for round {}...",
                node.lock().await.id, round
            );

            // Handle the result properly
            match send_proposals(&client, node.clone(), &shards, &merkle_root, parents).await {
                Ok(_) => {
                    info!(
                        "Node {}: Proposals for round {} sent successfully.",
                        node.lock().await.id, round
                    );
                }
                Err(e) => {
                    error!(
                        "Node {}: Failed to send proposals for round {}. Error: {:?}",
                        node.lock().await.id, round, e
                    );
                }
            }
        }
        Err(e) => {
            error!(
                "Node {}: Failed to create transaction data for round {}. Error: {:?}",
                node.lock().await.id, round, e
            );
        }
    }
}






/// **🕰️ Improved Commit Confirmation**
/// Waits until all expected units for the given round are present in the DAG.
async fn wait_for_commit_confirmation(node: Arc<Mutex<Node>>, round: usize) -> Result<(), anyhow::Error> {
    use tokio::time::{sleep, Duration};

    let max_retries = 30;
    let mut retries = 0;

    // Handle first round commit immediately.
    if round == 1 {
        info!("Node: Round 1 does not require DAG sync. Skipping.");
        return Ok(());
    }

    while retries < max_retries {
        let node_guard = node.lock().await;
        let dag = node_guard.dag.lock().await;

        let prev_round = round - 1;

        if let Some(units) = dag.get(&(prev_round as u64)) {
            if units.len() == node_guard.total_nodes {
                info!(
                    "Node {}: DAG confirmed for round {}. Proceeding to round {}.",
                    node_guard.id, prev_round, round
                );
                return Ok(());
            }
        }

        retries += 1;
        warn!(
            "Node {}: Waiting for DAG completion of round {}... (Attempt {}/{})",
            node_guard.id, prev_round, retries, max_retries
        );

        drop(dag);
        drop(node_guard);

        sleep(Duration::from_secs(5)).await;
    }

    error!(
        "Node {}: Timeout waiting for DAG round {} to complete.",
        node.lock().await.id, round - 1
    );
    Err(anyhow::anyhow!(
        "Timeout waiting for DAG round {} to complete",
        round - 1
    ))
}



