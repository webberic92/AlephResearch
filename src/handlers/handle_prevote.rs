use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::Digest;
use tokio::sync::RwLock;
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
        merkle_utils::{interpolate_shares, reconstruct_unit, validate_merkle_branch},
    },
};
use tokio::time::timeout;
use std::time::Duration;


/// Handles an incoming PREVOTE request in the ch-RBC protocol.
/// 
/// This function implements the key steps of ch-RBC as follows:
/// 
/// **ch-RBC Steps:**
/// - **Step 14**: Upon receiving `2f + 1` valid prevote messages, begin reconstruction.
/// - **Step 15**: Reconstruct the unit from the received shards.
/// - **Step 16**: If reconstruction fails (Merkle root mismatch), terminate.
/// - **Step 17**: Ensure all parents of the unit are locally available before proceeding.
/// - **Step 18**: Interpolate missing shards using `f+1` shares.
/// - **Step 19**: Compute the Merkle root from the interpolated shares.
/// - **Step 20**: If the computed root matches the expected root and commit has not been sent, proceed.
/// - **Step 21**: Multicast the commit message to finalize consensus.

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

    // --- Step 14: Ensure DAG round is synchronized before prevoting ---
    let epoch_id = prevote_request.propose.base.epoch_id;
    ensure_dag_round_sync(node.clone(), epoch_id).await?;

    // --- Decode Base64-encoded shards ---
    let decoded_shards = prevote_request.propose.shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

    // --- Decode Base64-encoded proofs ---
    let decoded_proofs = prevote_request.propose.proofs
        .iter()
        .map(|proof| proof.iter()
            .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
        )
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode proofs: {:?}", node_id, e))?;

    // --- Compute shard hashes ---
    let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // --- Step 16: Validate Merkle branches before proceeding ---
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            return Err(format!(
                "Node {}: Merkle root mismatch for shard {}.",
                node_id, index
            ));
        }
    }

    // --- Step 15: Reconstruct the unit from the received shards ---
    let reconstructed_unit = reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.epoch_id,
        prevote_request.propose.parents.clone(), // ✅ Pass the correct parents from the proposal
    )
    .map_err(|e| format!("Node {}: Reconstruction failed. Error: {:?}", node_id, e))?;

    // --- Step 17: Ensure all parent units are locally committed before proceeding ---
    {
        let node_read = node.read().await;

        if reconstructed_unit.epoch_id == 1 {
            info!("Node {}: First epoch detected, skipping parent commitment check.", node_id);
        } else {
            for parent in &prevote_request.propose.parents {
                if !node_read.is_unit_committed(parent).await {
                    return Err(format!(
                        "Node {}: Parent unit {} not received via RBC yet. Cannot commit.",
                        node_id, parent
                    ));
                }
            }
        }
    }
    
    //TODO make this work.
    // // --- Step 18: Interpolate missing shards using `f+1` shares (only if epoch > 1) ---
    // let interpolated_shards = if epoch_id > 1 {
    //     interpolate_shares(&decoded_shards, epoch_id).map_err(|e| {
    //         format!("Node {}: Failed to interpolate shares. Error: {:?}", node_id, e)
    //     })?
    // } else {
    //     info!("Epoch 1 detected: Skipping interpolation, using provided shards.");
    //     decoded_shards.clone() // ✅ Just use original shards
    // };


    // // --- Step 19: Compute new Merkle root from the interpolated shares ---
    // let new_merkle_root = sha2::Sha256::digest(&interpolated_shards.concat()).to_vec();

    // if new_merkle_root != prevote_request.propose.base.root {
    //     return Err(format!(
    //         "Node {}: Merkle root mismatch after interpolation. Cannot proceed to commit.",
    //         node_id
    //     ));
    // }

    info!(
        "Node {}: Checking quorum for epoch {}", node_id, epoch_id
    );

    // --- Step 14: Ensure `2f+1` valid prevotes before committing ---
    let f = node.read().await.get_fault_tolerance_threshold();
    let epoch_key = epoch_id.to_be_bytes().to_vec();

    let vote_count_result = timeout(Duration::from_secs(5), async {
        let node_read = node.read().await;
        let mut quorum_votes = node_read.quorum_votes.write().await;
        let count = quorum_votes.entry(epoch_key.clone()).or_insert(0);
        *count += 1;
        *count // Return updated vote count
    }).await;

    let vote_count = match vote_count_result {
        Ok(count) => count,
        Err(_) => {
            error!("Node {}: Quorum vote lock timed out!", node_id);
            return Err("Quorum vote lock timeout".to_string());
        }
    };

    info!(
        "Node {}: Checking quorum vote count {}/{}",
        node_id, vote_count, node.read().await.get_quorum_threshold()
    );

    if vote_count < node.read().await.get_quorum_threshold() {
        return Err(format!(
            "Node {}: Not enough prevotes received ({} / {}).",
            node_id, vote_count, node.read().await.get_quorum_threshold()
        ));
    }

    // --- Step 21: Multicast commit message to finalize consensus ---
    info!("Node {}: Quorum reached. Sending commit.", node_id);
    let encoded_proofs: Vec<Vec<String>> = decoded_proofs.iter()
    .map(|proof| proof.iter()
        .map(|p| general_purpose::STANDARD.encode(p)) // ✅ Convert `Vec<u8>` to `String`
        .collect()
    )
    .collect();

    let commit_request = CommitRequest {
        base: prevote_request.propose.base.clone(),
        unit: reconstructed_unit.data.clone(),
        proofs: decoded_proofs.iter()
            .map(|proof| proof.iter().map(|p| base64::engine::general_purpose::STANDARD.encode(p)).collect())
            .collect(),
        parents: prevote_request.propose.parents.clone(),
    };

    handle_commit(node.clone(), commit_request).await.map_err(|e| {
        error!(
            "Node {}: Commit phase failed for epoch {}. Error: {:?}",
            node_id, epoch_id, e
        );
        format!("Commit phase failed: {:?}", e)
    })?;

    info!("Node {}: Commit phase completed successfully. (from prevote)", node_id);
    
    // ✅ Clear quorum votes after commit
    {
        let node_write = node.write().await;
        let mut quorum_votes = node_write.quorum_votes.write().await;
        quorum_votes.remove(&epoch_key);
    }
    
    info!("Node {}: Prevote successfully handled for epoch {}.", node_id, epoch_id);
    Ok(())
}
