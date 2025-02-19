use base64::{engine::general_purpose, Engine};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::{error, info};
use reqwest::Client;
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{
        node::Node,
        requests::{CommitRequest, DagUnit, PrevoteRequest},
    },
    utils::merkle_utils::{compute_merkle_root, interpolate_shares, reconstruct_unit, validate_merkle_branch},
};

/*
**ch-RBC Proof Validation for `handle_prevote`**
--------------------------------------------------

**Step 14:** Upon receiving `2f + 1` valid `prevote(h, ·, ·)`
   - Count received `prevote` messages and check if the quorum threshold (`2f + 1`) is met.

**Step 15:** Reconstruct `U` from the received `s_j`
   - Use the received shards to reconstruct the proposed unit.

**Step 16:** Validate reconstructed `U`
   - If the reconstructed unit is invalid (e.g., invalid Merkle root or missing parents), terminate processing.

**Step 17:** Wait until all of `U`'s parents are locally available
   - Ensure that all parent units have been received and committed before proceeding.

**Step 18:** Interpolate `s_j` from `f + 1` shares
   - Perform interpolation on the shards if necessary to recover the original data.

**Step 19:** Compute Merkle root `h'` from interpolated shares
   - Generate a Merkle root from the interpolated shares to compare against the original.

**Step 20:** If `h = h'` and `commit(P_s, r, ·)` has not been sent, multicast commit
   - If the computed root matches the expected root, send `commit` messages to all nodes.

**Step 21:** Cleanup quorum votes after successful commit
   - Remove the quorum vote entry from the tracking map after a successful commit.
*/

/// Handles an incoming PREVOTE request in the ch-RBC protocol.
pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    info!("🔹 handle_prevote: Entering PREVOTE handler...");

    // **Step 14:** Extract necessary values while minimizing lock duration
    let (node_id, quorum_threshold, sender_url, round_id) = {
        let node_guard = node.lock().await;
        (
            node_guard.id,
            node_guard.get_quorum_threshold(),
            node_guard.ip_address.clone(),
            prevote_request.propose.base.round_id,
        )
    };

    info!(
        "🔹 Node {}: Handling PREVOTE request from Node {} for round {}",
        node_id, prevote_request.propose.base.proposing_node_id, round_id
    );

    // **Step 14:** Decode shards and validate Merkle proofs
    let decoded_shards: Vec<Vec<u8>> = prevote_request
        .propose
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec())
        .collect();

    for (index, proof) in prevote_request.propose.proofs.iter().enumerate() {
        let decoded_proof = proof
            .iter()
            .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<Vec<Vec<u8>>, _>>()
            .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;

        if !validate_merkle_branch(&shard_hashes, &decoded_proof, index, &prevote_request.propose.base.root) {
            return Err(format!("Node {}: Merkle root mismatch for shard {}", node_id, index));
        }
    }

    // **Step 15:** Reconstruct the unit (Now as `DagUnit`)
    let reconstructed_unit = reconstruct_unit(
        &decoded_shards,
        round_id,
        prevote_request.propose.parents.clone(),
        prevote_request.propose.base.proposing_node_id, // ✅ Use proposer ID from request
    )
    .map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

    if reconstructed_unit.transactions.is_empty() {
        return Err(format!("Node {}: Reconstructed unit is invalid or empty", node_id));
    }


    // **Step 17:** Ensure parent units are committed
    {
        let node_guard = node.lock().await;
        if round_id != 1 {
            for parent in &prevote_request.propose.parents {
                if !node_guard.is_unit_committed(parent).await {
                    return Err(format!("Node {}: Parent unit {} not committed yet.", node_id, parent));
                }
            }
        }
    }

    // **Step 18:** Interpolate missing shares (if needed)
    let interpolated_shards = if round_id > 1 {
        interpolate_shares(&decoded_shards, round_id)
            .map_err(|e| format!("Node {}: Failed to interpolate shares: {:?}", node_id, e))?
    } else {
        decoded_shards.clone()
    };

    // **Step 19:** Compute Merkle root from interpolated shares
    let new_merkle_root = compute_merkle_root(&interpolated_shards);

    if new_merkle_root != prevote_request.propose.base.root {
        return Err(format!("Node {}: Merkle root mismatch after interpolation.", node_id));
    }

    // **Step 14 (continued):** Count quorum votes
    let epoch_key = round_id.to_be_bytes().to_vec();
    let vote_count = {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let count = quorum_votes.entry(epoch_key.clone()).or_insert(0);
        *count += 1;
        *count
    };

    info!("🔹 Node {}: Quorum votes {}/{}", node_id, vote_count, quorum_threshold);

    if vote_count < quorum_threshold {
        info!("🔹 Node {}: Not enough prevote messages received. Waiting for quorum before proceeding to commit.", node_id);
        return Ok(());
    }

    // **Step 20:** Prepare commit request
    let commit_request = CommitRequest {
        base: prevote_request.propose.base.clone(),
        proofs: prevote_request.propose.proofs.clone(),
        parents: prevote_request.propose.parents.clone(),
        units: vec![reconstructed_unit.clone()],
    };

    // **Step 20 (continued):** Multicast commit messages
    let node_guard = node.lock().await;
    for target_node in &node_guard.nodes {
        let target_url = format!("http://{}/commit", target_node);
        let client = client.clone();
        let commit_payload = commit_request.clone();
        tokio::spawn(async move {
            if let Err(e) = client.post(&target_url).json(&commit_payload).send().await {
                error!("Failed to send commit to {}: {:?}", target_url, e);
            } else {
                info!("✅ Sent commit message to {}", target_url);
            }
        });
    }

    // **Step 21:** Trigger commit locally
    handle_commit(node.clone(), commit_request).await.map_err(|e| format!("Commit phase failed: {:?}", e))?;

    info!("✅ Node {}: Successfully processed PREVOTE for round {}.", node_id, round_id);
    Ok(())
}
