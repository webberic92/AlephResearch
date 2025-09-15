use tracing::{error, info};
use std::{sync:: Arc, time::Duration};
use tokio::{sync::Mutex, time::{sleep, timeout}};
use crate::{processors::priority_queue::RBCMessage, structs::{ node::Node, requests::ProposeRequest }};
use anyhow::anyhow;


pub async fn send_proposals(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), anyhow::Error> {
    //info!("🔍 [DEBUG] Attempting to acquire lock for send_proposals() in round {}", propose_request.base.round_id);
    let local_client = reqwest::Client::builder()
    .pool_max_idle_per_host(64)
    .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
    .build()
    .expect("Failed to build HTTP client");

    let lock_result = timeout(Duration::from_secs(5), node.lock()).await;
    
    let node_guard = match lock_result {
        Ok(guard) => {
            //info!("🔓 [DEBUG] Acquired node lock for send_proposals() in round {}", propose_request.base.round_id);
            guard
        }
        Err(_) => {
            error!("❌ [ERROR] Timeout acquiring node lock for send_proposals() in round {}. Possible deadlock!", propose_request.base.round_id);
            return Err(anyhow!("Timeout acquiring lock for send_proposals() in round {}", propose_request.base.round_id));
        }
    };

    let round_id = propose_request.base.round_id;
    let node_id = node_guard.id;
    let nodes = node_guard.nodes.clone();
    let rbc_processor = node_guard.rbc_processor.clone();  // ✅ Clone `rbc_processor` while holding the lock
    drop(node_guard); // 🔥 **Explicitly drop lock after extracting values**

    info!(
        "🚀 [DEBUG] Node {} preparing to send proposals to {:?} for round {}",
        node_id, nodes, round_id
    );


    let mut results: Vec<Result<(), anyhow::Error>> = Vec::new();

    for node_url in nodes {
        let proposal = propose_request.clone();
        let mut attempt = 0;
        let max_attempts = 3;
        let mut success = false;

        while attempt < max_attempts {
            attempt += 1;

            info!("📤 Attempt {}/{}: Sending proposal to {} (round {})", attempt, max_attempts, node_url, round_id);

            let res = local_client
                .post(format!("http://{}/propose", node_url))
                .json(&proposal)
                .timeout(Duration::from_millis(500))
                .send()
                .await;

            match res {
                Ok(res) if res.status().is_success() => {
                    info!("✅ Proposal delivered to {} (round {})", node_url, round_id);
                    success = true;
                    break;
                }
                Ok(res) => {
                    let status = res.status();
                    let msg = res.text().await.unwrap_or_else(|_| "No response".to_string());
                    error!("❌ Proposal failed to {} with status {}: {}", node_url, status, msg);
                }
                Err(e) => {
                    error!("❌ Network error to {}: {:?}", node_url, e);
                }
            }

            // 🔁 Wait before retrying (backoff)
            let delay = 100 * 2u64.pow((attempt - 1) as u32); // 100ms, 200ms, 400ms
            sleep(Duration::from_millis(delay)).await;
        }

        if success {
            results.push(Ok(()));
        } else {
            results.push(Err(anyhow!("Failed to send to {}", node_url)));
        }
    }
   

    if results.iter().all(|res| res.is_ok()) {
        info!("✅ Successfully sent all proposals for round {}.", round_id);

        // ✅ **Enqueue the proposal into the RBC queue**
        if let Some(rbc_processor) = rbc_processor {
            let proposal_message = RBCMessage::Proposal(propose_request.clone());
            info!("📥 [DEBUG] Node {} enqueuing proposal for round {} into RBCProcessor queue", node_id, round_id);
            rbc_processor.enqueue_message(proposal_message).await;
        } else {
            error!("❌ [ERROR] Node {}: RBCProcessor is not initialized when enqueuing proposal!", node_id);
            return Err(anyhow!("RBCProcessor not initialized when enqueuing proposal!"));
        }

        Ok(())
    } else {
        Err(anyhow!("❌ One or more proposals failed."))
    }
}




