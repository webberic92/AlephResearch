use base64::Engine;
use reqwest::Client;
use sha2::Digest;
use tokio::{sync::RwLock, time::timeout};
use std::{sync::Arc, time::Duration};
use tracing::{error, info, warn};
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest},
    },
    utils::{
        dag_utils::{ensure_dag_synchronization},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

/// Handles an incoming PREVOTE request in the ch-RBC protocol.
pub async fn handle_prevote(
    node: Arc<RwLock<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let node_id = node.read().await.id;

    info!(
        "Node {}: Handling PREVOTE request from Node {} for epoch {}",
        node_id, prevote_request.propose.base.proposing_node_id, prevote_request.propose.base.epoch_id
    );

    // --- Step 1: Validate the epoch ---
    let current_epoch = {
        let node_state = timeout(Duration::from_secs(5), node.read()).await
            .map_err(|_| format!("Node {}: Timeout while acquiring read lock in handle prevote Step 1!", node_id))?;

        let current_epoch_lock = node_state.current_epoch.lock().await;
        *current_epoch_lock
    };

    // --- Step 2: Decode Base64-encoded shards ---
    let decoded_shards = prevote_request.propose.shards
        .iter()
        .map(|shard| base64::engine::general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

    // --- Step 3: Decode Base64-encoded proofs ---
    let decoded_proofs = prevote_request.propose.proofs
        .iter()
        .map(|proof| proof.iter()
            .map(|p| base64::engine::general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
        )
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode proofs: {:?}", node_id, e))?;

    // --- Step 4: Ensure DAG synchronization ---
    // ensure_dag_synchronization(
    //     node.clone(),
    //     &client,
    //     prevote_request.propose.base.epoch_id,
    //     &prevote_request.propose.base.proposing_node_id,
    //     prevote_request.sender_url.clone(),
    // )
    // .await
    // .map_err(|e| {
    //     let err_msg = format!("Node {}: DAG synchronization failed. Error: {:?}", node_id, e);
    //     error!("{}", err_msg);
    //     err_msg
    // })?;

    // info!("Node {}: DAG synchronization successful.", node_id);

    // --- Step 5: Compute shard hashes ---
    let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // --- Step 6: Validate Merkle branches ---
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            return Err(format!(
                "Node {}: Merkle root mismatch for shard {}.",
                node_id, index
            ));
        }
    }

    // --- Step 7: Reconstruct the unit ---
    let parents = shard_hashes.iter().flat_map(|hash| hash.clone()).collect();
    let reconstructed_unit = reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.epoch_id,
        parents,
    )
    .map_err(|e| format!("Node {}: Reconstruction failed. Error: {:?}", node_id, e))?;

    // --- Step 8: Validate parents ---
    // validate_unit_parents(node.clone(), &reconstructed_unit.data).await.map_err(|e| {
    //     let err_msg = format!("Node {}: Parent validation failed. Error: {:?}", node_id, e);
    //     error!("{}", err_msg);
    //     err_msg
    // })?;

    // --- Step 9: Update quorum votes ---
    let should_commit = {
        let epoch_id = prevote_request.propose.base.epoch_id;
        let epoch_key = epoch_id.to_be_bytes().to_vec();

        let vote_count = {
            let node_state = timeout(Duration::from_secs(5), node.write()).await
                .map_err(|_| format!("Node {}: Timeout while acquiring write lock in prevote step 9!", node_id))?;

            let mut quorum_votes = node_state.quorum_votes.write().await;
            let vote_count = quorum_votes.entry(epoch_key.clone()).or_insert(0);
            *vote_count += 1;

            info!(
                "Node {}: Updated quorum votes for epoch {}. Current votes: {}",
                node_id, epoch_id, *vote_count
            );

            *vote_count
        };

        // let quorum_threshold = node.read().await.get_quorum_threshold();
        let is_quorum = node.read().await.is_quorum_reached(epoch_id).await;

        info!(
            "Node {}: Checking quorum for epoch {}, Current votes: {}",
            node_id, epoch_id, vote_count
        );

        is_quorum
    };

    // --- Step 10: Check quorum and handle commit ---
    if should_commit {
        info!(
            "Node {}: Quorum reached for epoch {}. Transitioning to commit.",
            node_id, prevote_request.propose.base.epoch_id
        );

        let commit_request = CommitRequest {
            base: prevote_request.propose.base.clone(),
            unit: reconstructed_unit.data.clone(),
            proofs: decoded_proofs.iter()
                .map(|proof| proof.iter().map(|p| base64::engine::general_purpose::STANDARD.encode(p)).collect())
                .collect(),
        };

        handle_commit(node.clone(), client.clone(), commit_request).await.map_err(|e| {
            let err_msg = format!(
                "Node {}: Commit phase failed for epoch {}. Error: {:?}",
                node_id, prevote_request.propose.base.epoch_id, e
            );
            error!("{}", err_msg);
            err_msg
        })?;
        
        let node_state = timeout(Duration::from_secs(5), node.write()).await
        .map_err(|_| format!("Node {}: Timeout while acquiring write lock in prevote step 9!", node_id))?;
        let mut quorum_votes = node_state.quorum_votes.write().await;
        quorum_votes.clear();

    } else {
        info!(
            "Node {}: Quorum not yet reached for epoch {}. Required: {}, Current votes: {}",
            node_id, prevote_request.propose.base.epoch_id,
            node.read().await.get_quorum_threshold(),
            node.read().await.quorum_votes.read().await
                .get(&prevote_request.propose.base.epoch_id.to_be_bytes().to_vec())
                .cloned().unwrap_or(0)
        );
    }

    // --- Step 11: Return success response ---
    info!(
        "Node {}: Prevote successfully handled for epoch {}.",
        node_id, prevote_request.propose.base.epoch_id
    );

    Ok(())
}
