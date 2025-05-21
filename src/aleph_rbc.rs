use aleph_research::controllers::api_routes::initialize_apis;
use aleph_research::processors::rbc_processor::RBCProcessor;
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::create_transaction_data::create_transaction_data;
use aleph_research::utils::start_util::wait_for_all_nodes_health;
use socket2::{Domain, Socket, Type};
use tokio::sync::Mutex;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;
use aleph_research::utils::config_util::load_config;
use aleph_research::structs::node::Node;
use anyhow::Result;
use std::net::TcpListener as StdTcpListener;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    
    let config = load_config(None);
    let addr = config.network.listen_address.parse::<SocketAddr>()?;

    // ✅ Step 1: Create `Node` **without `RBCProcessor` initially**
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

    // ✅ Step 2: Now that `Node` exists, create `RBCProcessor`
    let rbc_processor: Arc<RBCProcessor> = Arc::new(RBCProcessor::new(node.clone()));

    // ✅ Step 3: Attach `rbc_processor` to `Node`
    Node::set_rbc_processor(node.clone(),rbc_processor.clone()).await;

    // ✅ Step 4: Pass everything to the API
    let app = initialize_apis(node.clone(), rbc_processor.clone());

    // ✅ **Spawn Transaction Execution Logic**
    let node_clone = node.clone();
    info!("LATENCY START");
    tokio::spawn(async move {
        if let Err(e) = execute_transaction_logic(node_clone).await {
            error!("Transaction execution failed: {:?}", e);
        }
    });


    // ✅ Step 5: Create custom socket using `socket2`
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.set_nodelay(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    let std_listener: StdTcpListener = socket.into();
    let listener = TcpListener::from_std(std_listener)?;

    info!("✅ Custom socket listener created on {}", addr);
    info!("✅ Starting Axum server on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())

}


async fn execute_transaction_logic(
    node: Arc<Mutex<Node>>, 
) -> Result<(), anyhow::Error> {  
    wait_for_all_nodes_health(node.clone()).await?;  

    info!("All Nodes are healthy starting transaction execution.");
    // ✅ Create transaction proposal with multiple transactions
    match create_transaction_data(node.clone()).await {
        Ok(propose_request) => {
            let node_id = {
            let node_guard = node.lock().await;
            node_guard.id
            };
            let round = propose_request.base.round_id;

            info!("Node {}: Created proposal with {} transactions for round {}.", 
                node_id, propose_request.transactions.len(), round
            );

            // ✅ Step 2: Send proposal
            if let Err(e) = send_proposals(node.clone(), propose_request).await {
                error!(
                    "Node {}: Failed to send proposal for round {}. Error: {:?}",
                    node_id, round, e
                );
            } else {
                info!(
                    "Node {}: Proposal for round {} sent successfully.",
                    node_id, round
                );
            }
        }
        Err(e) => {
            let node_id = {
                //info!("🔍 [DEBUG] Waiting to acquire node lock for aleph rbc rs");
                let node_guard = node.lock().await;
                //info!("🔓 [DEBUG] Acquired node lock for  aleph rbc rs");
                node_guard.id
            };
            error!(
                "Node {}: Failed to create transaction proposal. Error: {:?}",
                node_id, e
            );
        }
    }
    Ok(())
}
