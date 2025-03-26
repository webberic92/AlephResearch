use base64::{engine::general_purpose, Engine};
use futures::future::join_all;
use sha2::{Digest, Sha256};
use tokio::{sync::Mutex, time::{sleep, timeout}};
use std::{collections::HashSet, sync::{atomic::Ordering, Arc}, time::Duration};
use tracing::{error, info, warn};
use reqwest::Client;
use crate::{
    processors::priority_queue::RBCMessage, structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest, Transaction},
    }, utils::merkle_utils::{compute_merkle_root, interpolate_shares, reconstruct_unit, validate_merkle_branch}
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


// **Handles an incoming PREVOTE request with multiple proposals**

// **Handles an incoming PREVOTE request with multiple proposals**
pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,  
) -> Result<(), String> {
    info!(
        "handle_prevote: Processing {} proposals from {} for round {}",
        prevote_request.proposals.len(),
        prevote_request.sender_url,
        prevote_request.proposals[0].base.round_id
    );

    // Extract values **without holding the lock long**
    let (node_id, round_id, quorum_threshold, total_nodes, node_list, rbc_processor) = {
        //info!("🔍 [DEBUG] Waiting to acquire node lock for handle prevote");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for handle prevote");
                // ✅ Access message_count through the already locked `node_guard`
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.total_nodes,
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
            
        )
    }; // ✅ Release lock immediately

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voter_set = quorum_votes.entry(round_id.to_be_bytes().to_vec()).or_insert_with(HashSet::new);
        // Check if this node already voted
        if voter_set.contains(&prevote_request.sender_url) {
            info!(
                "Node {}: Already received prevote from Node {} for round {}. Ignoring.",
                node_id, prevote_request.sender_url, round_id
            );
            return Ok(());
        }
    } 

    let mut reconstructed_units = Vec::new();

    // ✅ **Process each proposal separately**
    for proposal in &prevote_request.proposals {
        // info!("Processing proposal from Node {} for round {}", proposal.base.proposing_node_id, round_id);
        let mut reconstructed_transactions = Vec::new();

        for transaction in &proposal.transactions {
            let decoded_shards: Vec<Vec<u8>> = transaction.shards
                .iter()
                .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
                .collect::<Result<Vec<Vec<u8>>, _>>()
                .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

            let shard_hashes: Vec<Vec<u8>> = decoded_shards.iter()
                .map(|shard| Sha256::digest(shard).to_vec())
                .collect();

            for (shard_index, _) in decoded_shards.iter().enumerate() {
                let decoded_proof: Vec<Vec<u8>> = transaction.proofs[shard_index]
                    .iter()
                    .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
                    .collect::<Result<Vec<Vec<u8>>, _>>()
                    .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;

                if !validate_merkle_branch(&shard_hashes[shard_index], &decoded_proof, shard_index, &transaction.root) {
                    return Err(format!(
                        "Node {}: Merkle root mismatch for shard {}",
                        node_id, shard_index
                    ));
                }
            }

            let interpolated_shards = if decoded_shards.len() < total_nodes {
                interpolate_shares(&decoded_shards, round_id)
                    .map_err(|e| format!("Node {}: Failed to interpolate shares: {:?}", node_id, e))?
            } else {
                decoded_shards.clone()
            };

            let interpolated_shard_hashes: Vec<Vec<u8>> = interpolated_shards.iter()
                .map(|shard| Sha256::digest(shard).to_vec())
                .collect();

            let new_merkle_root = compute_merkle_root(&interpolated_shard_hashes);

            if new_merkle_root != transaction.root {
                return Err(format!(
                    "Node {}: Merkle root mismatch after interpolation. Expected {:?} but got {:?}",
                    node_id, transaction.root, new_merkle_root
                ));
            }

            let reconstructed_tx = Transaction {
                root: transaction.root.clone(),
                proofs: transaction.proofs.clone(),
                shards: interpolated_shards.iter().map(|s| String::from_utf8_lossy(s).to_string()).collect(), 
            };

            reconstructed_transactions.push(reconstructed_tx);
        }

        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,
            round_id,
            proposal.parents.clone(),
            proposal.base.proposing_node_id as usize,
        ).map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

        if reconstructed_unit.transactions.is_empty() {
            return Err(format!("Node {}: Reconstructed unit is invalid or empty", node_id));
        }

        reconstructed_units.push(reconstructed_unit);
    }


    // 👇 Ensure that `prevote_request` contains the sender's node ID
    let sender_id = prevote_request.sender_url;
    let round_key = round_id.to_be_bytes().to_vec();
    let vote_count;

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;

        // Get or insert a new HashSet for this round
        let voter_set = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);

        // Insert the new vote
        voter_set.insert(sender_id);
        vote_count = voter_set.len();
    }

    if vote_count < quorum_threshold {
        info!(
            "Node {}: Not enough prevote messages received ({}/{}). Waiting for quorum before proceeding to commit.",
            node_id, vote_count, quorum_threshold
        );
        return Ok(());
    }

    info!("Node {}: Quorum reached for round {}. Proceeding to send commits.", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone(); // ✅ Clone the Arc<AtomicU64>

    for target_node in node_list {
        let target_url = format!("http://{}/commit", target_node);
        let commit_payload = commit_request.clone();
    
        message_count.fetch_add(1, Ordering::Relaxed);
    
        match client
            .post(&target_url)
            .json(&commit_payload)
            .timeout(Duration::from_millis(500))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                info!("✅ Successfully sent commit to {}", target_url);
            }
            Ok(response) => {
                error!("❌ Commit failed to {}. Status: {:?}", target_url, response);
            }
            Err(e) => {
                error!("❌ Network error while sending commit to {}: {:?}", target_url, e);
            }
        }
    }
    
    if let Some(rbc_processor) = &rbc_processor {
        info!("Node {}: Adding commit to queue for round {}", node_id, round_id);
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request)).await;
    } else {
        error!("Node {}: RBCProcessor not initialized when trying to enqueue commit!", node_id);
    }

    Ok(())
}

