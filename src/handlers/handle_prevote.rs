use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::{error, info};
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest},
    },
    utils::{
        dag_utils::ensure_dag_round_sync,
        merkle_utils::{compute_merkle_root, interpolate_shares, reconstruct_unit, validate_merkle_branch},
    },
};

/// Handles an incoming PREVOTE request in the ch-RBC protocol.
///
/// **ch-RBC Steps:**  
/// - **Step 14:** Check quorum for `2f + 1` valid prevote messages.  
/// - **Step 15:** Reconstruct the unit from shards.  
/// - **Step 16:** Validate the Merkle root.  
/// - **Step 17:** Ensure parent units are committed.  
/// - **Step 18:** Interpolate missing shares if necessary.  
/// - **Step 19:** Compute Merkle root and compare with the expected.  
/// - **Step 20:** If root matches, send commit messages.  
/// - **Step 21:** Clean up quorum votes afterward.
pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    // Step 1: Log and Extract Node ID
    let node_id = {
        let node_guard = node.lock().await;
        node_guard.id
    };

    info!(
        "Node {}: =============Handling PREVOTE request from Node {} for round {} =================",
        node_id, 
        prevote_request.propose.base.proposing_node_id, 
        prevote_request.propose.base.round_id
    );

    // Step 2: Decode and Validate Shards
    let decoded_shards = prevote_request.propose.shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

    let decoded_proofs = prevote_request.propose.proofs
        .iter()
        .map(|proof| proof.iter()
            .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
        )
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode proofs: {:?}", node_id, e))?;

    let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    // Step 3: Validate Merkle Branches
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            return Err(format!("Node {}: Merkle root mismatch for shard {}", node_id, index));
        }
    }

    // Step 4: Reconstruct the Unit
    let reconstructed_unit = reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.round_id,
        prevote_request.propose.parents.clone(),
    )
    .map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

    // Step 5: Validate Parent Commitments
    {
        let node_guard = node.lock().await;
        if reconstructed_unit.round_id != 1 {
            for parent in &prevote_request.propose.parents {
                if !node_guard.is_unit_committed(parent).await {
                    return Err(format!("Node {}: Parent unit {} not committed yet.", node_id, parent));
                }
            }
        }
    }

    // Step 6: Interpolate Shares if Needed
    let interpolated_shards = if prevote_request.propose.base.round_id > 1 {
        interpolate_shares(&decoded_shards, prevote_request.propose.base.round_id)
            .map_err(|e| format!("Node {}: Failed to interpolate shares: {:?}", node_id, e))?
    } else {
        decoded_shards.clone()
    };

    // Step 7: Compute Merkle Root and Validate
    let interpolated_hashes: Vec<Vec<u8>> = interpolated_shards.iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    let new_merkle_root = compute_merkle_root(&interpolated_hashes);
    if new_merkle_root != prevote_request.propose.base.root {
        return Err(format!("Node {}: Merkle root mismatch after interpolation.", node_id));
    }

    // Step 8: Check Quorum
    let round_id = prevote_request.propose.base.round_id;
    let epoch_key = round_id.to_be_bytes().to_vec();

    let vote_count = {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let count = quorum_votes.entry(epoch_key.clone()).or_insert(0);
        *count += 1;
        *count
    };

    let quorum_threshold = {
        let node_guard = node.lock().await;
        node_guard.get_quorum_threshold()
    };

    info!("Node {}: Quorum votes {}/{}", node_id, vote_count, quorum_threshold);

    if vote_count < quorum_threshold {
        return Err(format!(
            "Node {}: Not enough prevotes received: {}/{}",
            node_id, vote_count, quorum_threshold
        ));
    }

    // Step 9: Construct Commit Request and Trigger Commit
    let commit_request = CommitRequest {
        base: prevote_request.propose.base.clone(),
        unit: reconstructed_unit.data.clone(),
        proofs: decoded_proofs.iter()
            .map(|proof| proof.iter().map(|p| general_purpose::STANDARD.encode(p)).collect())
            .collect(),
        parents: prevote_request.propose.parents.clone(),
    };

    handle_commit(node.clone(), commit_request).await.map_err(|e| {
        error!("Node {}: Commit phase failed: {:?}", node_id, e);
        format!("Commit phase failed: {:?}", e)
    })?;

    // Step 10: Clear Quorum Votes After Success
    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        quorum_votes.remove(&epoch_key);
    }

    // ✅ Success
    info!("Node {}: Prevote successfully handled for round {}.", node_id, round_id);
    Ok(())
}
