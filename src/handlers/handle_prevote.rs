use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::{error, info};
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{
        node::Node,
        requests::{CommitRequest, DagUnit, PrevoteRequest},
    },
    utils::merkle_utils::{reconstruct_unit, validate_merkle_branch,
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
/// Handles an incoming PREVOTE request in the ch-RBC protocol.
pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let node_id = {
        let node_guard = node.lock().await;
        node_guard.id
    };

    info!(
        "Node {}: Handling PREVOTE request from Node {} for round {}",
        node_id,
        prevote_request.propose.base.proposing_node_id,
        prevote_request.propose.base.round_id
    );

    // Decode shards
    let decoded_shards = prevote_request.propose.shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

    // Validate Merkle proofs
    let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    for (index, proof) in prevote_request.propose.proofs.iter().enumerate() {
        let decoded_proof = proof.iter()
            .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<Vec<u8>>, _>>()
            .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;

        if !validate_merkle_branch(&shard_hashes, &decoded_proof, index, &prevote_request.propose.base.root) {
            return Err(format!("Node {}: Merkle root mismatch for shard {}", node_id, index));
        }
    }

    // Reconstruct unit
    let reconstructed_unit = reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.round_id,
        prevote_request.propose.parents.clone(),
    )
    .map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

    // Validate parent commitments
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

    // Check quorum
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
        return Ok(());
    }

    // Collect all units for this round
    let units_for_commit = {
        let node_guard = node.lock().await;
        let dag = node_guard.dag.lock().await;

        let mut units: Vec<DagUnit> = Vec::new();
        if let Some(round_units) = dag.get(&round_id) {
            units.extend(round_units.clone());
        }
        units
    };

    if units_for_commit.len() < quorum_threshold {
        info!("Node {}: Not enough units to commit yet.", node_id);
        return Ok(());
    }

    // Prepare commit request
    let commit_request = CommitRequest {
        base: prevote_request.propose.base.clone(),
        proofs: prevote_request.propose.proofs.clone(),
        parents: prevote_request.propose.parents.clone(),
        units: units_for_commit,
    };

    // Trigger commit once with all units
    handle_commit(node.clone(), commit_request).await.map_err(|e| {
        error!("Node {}: Commit phase failed: {:?}", node_id, e);
        format!("Commit phase failed: {:?}", e)
    })?;

    // Clear votes after commit
    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        quorum_votes.remove(&epoch_key);
    }

    info!("Node {}: Prevote successfully handled for round {}.", node_id, round_id);
    Ok(())
}

