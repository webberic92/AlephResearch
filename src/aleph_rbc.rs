use aleph_research::utils::epoch_utils::handle_sync_epoch;
use axum::{routing::post, Json, Router};
use reqwest::Client;
use std::{
    net::SocketAddr,
    sync::Arc,
};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber;

use aleph_research::utils::config_util::load_config;
use aleph_research::handlers::handle_prevote::handle_prevote;
use aleph_research::handlers::handle_propose::handle_propose;
use aleph_research::handlers::handle_commit::handle_commit;

use aleph_research::structs;
use structs::requests::{ProposeRequest, PrevoteRequest, CommitRequest, SyncEpochRequest};
use structs::responses::Response;
use structs::node::Node;


fn initialize_apis(node: Arc<Node>, client: Arc<Client>) -> Router {
    Router::new()
        .route("/propose", post({
            let node = node.clone();
            let client = client.clone();
            move |Json(payload): Json<ProposeRequest>| {
                let node = node.clone();
                let client: Arc<Client> = client.clone();
                async move {
                    handle_propose(
                        &node,
                        &client,
                        payload.sender,
                        payload.root,
                        payload.proof,
                        payload.shard,
                        payload.epoch_id,
                    )
                    .await;
                    Json(Response {
                        status: format!(
                            "Node {}: Propose accepted from Node {} for epoch {}",
                            node.id, payload.sender, payload.epoch_id
                        ),
                    })
                }
            }
        }))
        .route("/prevote", post({
            let node = node.clone();
            move |Json(payload): Json<PrevoteRequest>| {
                let node = node.clone();
                async move {
                    handle_prevote(&node, payload.sender, payload.root, payload.epoch_id).await;
                    Json(Response {
                        status: format!(
                            "Node {}: Prevote accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
        .route("/commit", post({
            let node = node.clone();
            move |Json(payload): Json<CommitRequest>| {
                let node = node.clone();
                async move {
                    handle_commit(&node, payload.sender, payload.root).await;
                    Json(Response {
                        status: format!(
                            "Node {}: Commit accepted from Node {}",
                            node.id, payload.sender
                        ),
                    })
                }
            }
        }))
        .route("/sync_epoch", post({
            let node = node.clone();
            move |Json(payload): Json<SyncEpochRequest>| {
                let node = node.clone();
                info!("Node {}: ==== Handling SNYC EPOCH request from Node {} ====", node.id, payload.sender);
                async move {
                    match handle_sync_epoch(&node, payload.epoch_id, payload.sender).await {
                        Ok(_) => Json(Response {
                            status: format!(
                                "Node {}: Epoch {} synchronized successfully from {}",
                                node.id, payload.epoch_id , payload.sender
                            ),
                        }),
                        Err(e) => Json(Response {
                            status: format!(
                                "Node {}: Failed to synchronize epoch {} from {}: {}",
                                node.id, payload.epoch_id,payload.sender,e
                            ),
                        }),
                    }
                }
            }
        }))
        .route("/health", axum::routing::get({
            let node = node.clone();
            move || async move {
                let quorum_votes = node.quorum_votes.read().await;
                let epoch_tracker = node.epoch_tracker.lock().await;

                Json(Response {
                    status: format!(
                        "Node {} is healthy. Quorum votes: {:?}, Epochs: {:?}",
                        node.id, *quorum_votes, *epoch_tracker
                    ),
                })
            }
        }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    let addr = config.network.listen_address.parse::<SocketAddr>()?;
    let client = Arc::new(Client::new());

    let node = Arc::new(Node::new(config.node.id, config.node.total_nodes));
    let app = initialize_apis(node, client);

    let listener = TcpListener::bind(addr).await?;
    info!("API server running on {}", addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
