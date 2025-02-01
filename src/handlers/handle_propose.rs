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
    let node_read = node.read().await; // Acquire the read lock once
    let node_id = node_read.id;
    
    // Acquire the epoch lock and retrieve its value
    let node_epoch = *node_read.current_epoch.lock().await;
    drop(node_read); // Explicitly drop the read lock before acquiring the write lock
    
    // Safely acquire a write lock with a timeout
    let node_state = match timeout(Duration::from_secs(5), node.write()).await {
        Ok(state) => state,
        Err(_) => {
            error!("Node {}: Timeout while acquiring write lock!", node_id);
            return Err("Timeout while acquiring write lock".to_string());
        }
    };
    
    // Log proposal handling
    info!(
        "*** Handling PROPOSE REQUEST: Node {} from Sender {} for epoch {} ***",
        node_id, propose_request.base.proposing_node_id, propose_request.base.epoch_id
    );
    
    if propose_request.base.epoch_id < node_epoch {
        return Err(format!(
            "Outdated epoch {}, Node {} is on epoch {}",
            propose_request.base.epoch_id, node_id, node_epoch
        ).into());
    }
    
    // Release the write lock before performing any asynchronous operations
    drop(node_state);
    
    // --- Step 1: Decode Base64-encoded shards ---
    let decoded_shards: Vec<Vec<u8>> = propose_request
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| {
            let error_message = format!("Failed to decode shards: {:?}", e);
            error!("{}", error_message);
            error_message
        })?;

    // --- Step 2: Decode Base64-encoded proofs ---
    let decoded_proofs: Vec<Vec<Vec<u8>>> = propose_request
        .proofs
        .iter()
        .map(|proof| {
            proof
                .iter()
                .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
                .collect::<Result<Vec<_>, _>>() 
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            let error_message = format!("Failed to decode proofs: {:?}", e);
            error!("{}", error_message);
            error_message
        })?;

    // --- Step 3: Compute shard hashes ---
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // --- Step 4: Validate Merkle branches ---
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &propose_request.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}",
                node_id, index, propose_request.base.root
            );
            error!("{}", error_message);
            return Err(error_message);
        }
    }

    info!("Node {}: All Merkle branches validated successfully.", node_id);

    // --- Step 5: Update proposal tracker ---
    let proposal_count;
    let required_proposals;
    {
        // Safely acquire a write lock with a timeout
        let node_state = match timeout(Duration::from_secs(5), node.write()).await {
            Ok(state) => state,
            Err(_) => {
                error!("Node {}: Timeout while acquiring write lock!", node_id);
                return Err("Timeout while acquiring write lock".to_string());
            }
        };

        let mut proposal_tracker = node_state.proposal_tracker.lock().await;
        proposal_tracker.insert(propose_request.base.proposing_node_id);

        let node_count = node_state.total_nodes;
        let f = node_state.get_fault_tolerance_threshold();
        required_proposals = node_count - f;

        proposal_count = proposal_tracker.len();

        info!(
            "Node {}: Updated proposal tracker. Current proposals: {}/{}",
            node_state.id, proposal_count, required_proposals
        );
    }

    // --- Step 6: Prevote transition (if quorum is reached) ---
    if proposal_count > 0 {
        info!(
            "Node {}: Received enough proposals ({}/{}) for epoch {}. Transitioning to prevote.",
            node_id, proposal_count, required_proposals, propose_request.base.epoch_id
        );

        // Re-acquire write lock to prepare prevote request
        let node_state = match timeout(Duration::from_secs(5), node.write()).await {
            Ok(state) => state,
            Err(_) => {
                error!("Node {}: Timeout while acquiring write lock!", node_id);
                return Err("Timeout while acquiring write lock".to_string());
            }
        };

        let prevote_request = PrevoteRequest {
            propose: propose_request.clone(),
            sender_url: node_state.ip_address.clone(),
        };

        drop(node_state); // Release lock before async call

        info!(
            "Node {}: Sending prevote for epoch {} from sender {}",
            node_id, prevote_request.propose.base.epoch_id, prevote_request.propose.base.proposing_node_id
        );

        if let Err(e) = handle_prevote(node.clone(), client.clone(), prevote_request).await {
            error!(
                "Node {}: Failed to handle prevote for epoch {}. Error: {:?}",
                node_id, propose_request.base.epoch_id, e
            );
        } else {
            info!("Node {}: Prevote request successfully sent!", node_id);
        }
    } else {
        info!(
            "Node {}: Waiting for more proposals ({}/{}) for epoch {}.",
            node_id, proposal_count, required_proposals, propose_request.base.epoch_id
        );
    }

    info!(
        "Node {}: Proposal successfully handled for epoch {} from sender {}",
        node_id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
    );
    Ok(())
}
