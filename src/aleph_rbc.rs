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
        let prev_round = (round as u64) - 1;
        loop {
            let dag = {
                let node_guard = node.lock().await;
                let dag = node_guard.dag.lock().await;
                dag.contains_key(&prev_round)
            };
            if dag {
                break;
            }
            let node_id = node.lock().await.id;
            info!(
                "Node {}: Waiting for DAG round {} to be committed before sending round {} proposals...",
                node_id, round - 1, round
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    let node_id = node.lock().await.id;
    info!(
        "Node {}: DAG ready. Proceeding to round {} proposal dispatch.",
        node_id, round
    );

    // 🎯 Step 1: Create transaction data for this round
    info!("Node: Creating transaction data for round {}...", round);
    match create_transaction_data(node.clone()).await {
        Ok((shards, merkle_root, parents)) => {
            info!("Node: Transaction data created for round {}.", round);

            // 🎯 Step 2: Send proposals for this round
            info!("Node {}: Sending proposals for round {}...", node_id, round);
            if let Err(e) = send_proposals(&client, node.clone(), &shards, &merkle_root, parents).await {
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
            error!(
                "Node {}: Failed to create transaction data for round {}. Error: {:?}",
                node_id, round, e
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

    let prev_round = (round as u64) - 1;

    while retries < max_retries {
        let (dag_status, commit_quorum, node_id) = {
            let node_guard = node.lock().await;
            let dag = node_guard.dag.lock().await;

            let commit_quorum = node_guard.get_quorum_threshold();
            let node_id = node_guard.id;
            let units = dag.get(&prev_round);
            let received_units = units.map(|u| u.len()).unwrap_or(0);

            let dag_ready = received_units >= commit_quorum;

            (dag_ready, commit_quorum, node_id)
        };

        if dag_status {
            info!(
                "Node {}: DAG confirmed for round {}. Proceeding to round {}.",
                node_id, prev_round, round
            );

            let mut node_guard = node.lock().await;
            let mut current_round = node_guard.current_round.lock().await;
            if *current_round == prev_round {
                *current_round += 1;
                info!("Node {}: Local round advanced to {}", node_id, *current_round);
            }
            return Ok(());
        } else {
            warn!(
                "Node {}: Waiting for DAG completion of round {}... Retries: {}/{}",
                node_id, prev_round, retries + 1, max_retries
            );
        }

        retries += 1;
        sleep(Duration::from_secs(5)).await;
    }

    let node_id = node.lock().await.id;
    error!(
        "Node {}: Timeout waiting for DAG round {} to complete.",
        node_id, prev_round
    );
    Err(anyhow::anyhow!(
        "Timeout waiting for DAG round {} to complete",
        prev_round
    ))
}




