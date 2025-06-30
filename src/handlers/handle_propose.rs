use std::{
    sync::{atomic::Ordering, Arc},
    thread::sleep,
    time::Duration,
};
use tokio::{sync::Mutex, time::timeout};
use tracing::error;

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}},
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;

    // Early skip if already seen this proposal
    {
        let node_guard = node.lock().await;
        let proposal_tracker = node_guard.proposal_tracker.lock().await;
        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            if round_proposals.contains_key(&proposer_id) {
                return Ok(());
            }
        }

        let quorum_threshold = node_guard.total_nodes - node_guard.get_fault_tolerance_threshold();
        if let Some(round_map) = proposal_tracker.get(&round_id) {
            if round_map.len() >= quorum_threshold {
                return Ok(());
            }
        }
    }

    // Register proposal and check quorum
    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request).await?;

    if proposal_count < quorum_threshold {
        return Ok(());
    }

    // PrevoteRequest after quorum
    let prevote_request = {
        let node_guard = node.lock().await;
        PrevoteRequest {
            proposals: stored_proposals.clone(),
            sender_url: node_guard.ip_address.clone(),
            sender_id: node_guard.id,
        }
    };

    let (node_ip, node_id, node_list) = {
        let node_guard = node.lock().await;
        (node_guard.ip_address.clone(), node_guard.id, node_guard.nodes.clone())
    };

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .build()
        .expect("HTTP client build failed");

    for target in node_list {
        if target != node_ip {
            let url = format!("http://{}/prevote", target);
            let mut attempts = 0;
            let max_attempts = 3;
            let timeout_duration = Duration::from_secs(3);
            let mut success = false;

            while attempts < max_attempts {
                attempts += 1;
                let send_fut = client.post(&url).json(&prevote_request).send();

                match timeout(timeout_duration, send_fut).await {
                    Ok(Ok(resp)) if resp.status().is_success() => {
                        success = true;
                        break;
                    }
                    _ => {
                        let delay = 100 * 2u64.pow((attempts - 1) as u32);
                        sleep(Duration::from_millis(delay));
                    }
                }
            }

            if !success {
                error!("Node {}: Failed to send prevote to {} after {} attempts", node_id, url, max_attempts);
            }

            node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    {
        let node_guard = node.lock().await;
        if let Some(rbc_processor) = &node_guard.rbc_processor {
            rbc_processor.enqueue_message(RBCMessage::Prevote(prevote_request)).await;
        } else {
            error!("Node {}: RBCProcessor not initialized", node_guard.id);
        }
    }

    Ok(())
}

