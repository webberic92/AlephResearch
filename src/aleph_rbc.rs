use aleph_research::controllers::api_routes::initialize_apis;
use reqwest::Client;
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber;
use aleph_research::utils::config_util::load_config;
use aleph_research::structs;
use structs::node::Node;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    let client = Arc::new(Client::new());

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes, config.network.ip_address));
    let app = initialize_apis(node, client);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
