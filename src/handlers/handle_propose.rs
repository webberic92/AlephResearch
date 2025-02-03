use std::{sync::Arc, time::Duration};
use base64::{engine::general_purpose, Engine};
use sha2::Digest;
use tokio::{sync::RwLock, time::timeout};
use tracing::{error, info};
use reqwest::Client;
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{
        node::Node,
        requests::{PrevoteRequest, ProposeRequest},
    },
    utils::merkle_utils::validate_merkle_branch,
};

/// Handles an incoming proposal request in the ch-RBC protocol.
pub async fn handle_propose(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let node_id;
    let node_epoch;

    // ✅ Step 1: Read node state first, NO LOCKING
    {
        let node_read = node.read().await;
        node_id = node_read.id;
        node_epoch = *node_read.current_epoch.lock().await;
        info!(
            "Node {}: Handling Propose for epoch {} from sender {}",
            node_id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
        );
    } // 🔴 Drop read lock immediately

    if propose_request.base.epoch_id < node_epoch {
        return Err(format!(
            "Node {}: Outdated epoch {} received, current epoch is {}",
            node_id, propose_request.base.epoch_id, node_epoch
        ));
    }

    // ✅ Step 2: Decode shards OUTSIDE any lock
    let decoded_shards: Vec<Vec<u8>> = propose_request
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Failed to decode shards: {:?}", e))?;

    let decoded_proofs: Vec<Vec<Vec<u8>>> = propose_request
        .proofs
        .iter()
        .map(|proof| proof.iter().map(|p| general_purpose::STANDARD.decode(p.as_bytes())).collect::<Result<Vec<_>, _>>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to decode proofs: {:?}", e))?;

    // ✅ Step 3: Validate Merkle branches OUTSIDE lock
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &propose_request.base.root) {
            return Err(format!(
                "Node {}: Merkle root mismatch for shard {}.",
                node_id, index
            ));
        }
    }

    info!("Node {}: All Merkle branches validated successfully.", node_id);

    // ✅ Step 4: Acquire a write lock **ONLY FOR SHORT UPDATE**
    let proposal_count;
    let required_proposals;
    {
        info!("Node {}: Attempting to acquire write lock for proposal tracker update...", node_id);

        let mut node_write = match timeout(Duration::from_secs(5), node.write()).await {
            Ok(state) => state,
            Err(_) => {
                error!("Node {}: Timeout while acquiring write lock!", node_id);
                return Err("Timeout while acquiring write lock".to_string());
            }
        };

        let mut proposal_tracker = node_write.proposal_tracker.lock().await;
        proposal_tracker.insert(propose_request.base.proposing_node_id);
        proposal_count = proposal_tracker.len();

        let node_count = node_write.total_nodes;
        let f = node_write.get_fault_tolerance_threshold();
        required_proposals = node_count - f;
    } // 🔴 Drop locks immediately

    if proposal_count >= 1 {
        info!(
            "Node {}: Received enough proposals ({}/{}) for epoch {}. Transitioning to prevote.",
            node_id, proposal_count, required_proposals, propose_request.base.epoch_id
        );

        let prevote_request = {
            let node_read = node.read().await;
            PrevoteRequest {
                propose: propose_request.clone(),
                sender_url: node_read.ip_address.clone(),
            }
        };

        // ❌ BEFORE: Errors were lost
        // if let Err(e) = handle_prevote(node.clone(), client.clone(), prevote_request).await {
        //     error!("Node {}: Failed to handle prevote for epoch {}. Error: {:?}", node_id, propose_request.base.epoch_id, e);
        // }

        // ✅ FIXED: Return error properly if prevote (or commit) fails
        handle_prevote(node.clone(), client.clone(), prevote_request).await.map_err(|e| {
            error!(
                "Node {}: Failed to handle prevote for epoch {}. Error: {:?}",
                node_id, propose_request.base.epoch_id, e
            );
            format!("Prevote phase failed: {:?}", e)
        })?;
    }

    info!(
        "Node {}: Proposal successfully handled for epoch {} from sender {}",
        node_id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
    );
    Ok(())
}

