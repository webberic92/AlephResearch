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
    }, utils::merkle_utils::{reconstruct_unit, validate_merkle_branch}
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


    let (node_id, round_id, quorum_threshold, total_nodes, node_list, rbc_processor) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.total_nodes,
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
            
        )
    }; 

    // ✅ Early return if quorum has already been reached for this round
    {
        let node_guard = node.lock().await;
        let quorum_votes = node_guard.quorum_votes.lock().await;
        let round_key = round_id.to_be_bytes().to_vec();

        if let Some(voter_set) = quorum_votes.get(&round_key) {
            if voter_set.len() >= quorum_threshold {
                info!(
                    "Node {}: Quorum already reached for round {} ({} votes). Dropping incoming prevote.",
                    node_guard.id, round_id, voter_set.len()
                );
                return Ok(());
            }
        }
    }



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

        // Assumes transaction.root is SHA256(padded_tx) from proposal phase
        let batch_root = &proposal.batch_root;
        let batch_proofs = &proposal.batch_proofs;

        for (i, transaction) in proposal.transactions.iter().enumerate() {
            // Reconstruct raw tx padded to 250 bytes
            let padded_tx_bytes = match transaction.shards.first() {
                Some(shard_str) => {
                    let decoded = general_purpose::STANDARD
                        .decode(shard_str.as_bytes())
                        .map_err(|e| format!("Node {}: Failed to decode tx shard: {:?}", node_id, e))?;

                    if decoded.len() != 250 {
                        return Err(format!("Node {}: Transaction {} is not 250 bytes after decoding", node_id, i));
                    }

                    decoded
                },
                None => return Err(format!("Node {}: No transaction data provided in shards", node_id)),
            };

            let hash = Sha256::digest(&padded_tx_bytes).to_vec();

            if hash != transaction.root {
                return Err(format!(
                    "Node {}: Hash mismatch for transaction {}. Expected {:?}, got {:?}",
                    node_id,
                    i,
                    hex::encode(&transaction.root),
                    hex::encode(&hash)
                ));
            }

            let proof = &batch_proofs[i];

            if !validate_merkle_branch(&transaction.root, proof, i, batch_root) {
                return Err(format!(
                    "Node {}: Invalid Merkle proof for transaction {} in batch",
                    node_id, i
                ));
            }

            reconstructed_transactions.push(Transaction {
                root: transaction.root.clone(),
                proofs: vec![batch_proofs[i]
                    .iter()
                    .map(|b| hex::encode(b))  // convert bytes → hex string
                    .collect()],
                shards: transaction.shards.clone(), // retain original base64 string
            });
        }

        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,
            round_id,
            proposal.parents.clone(),
            proposal.base.proposing_node_id as usize,
            batch_root.clone(), // ✅ pass in the batch root
        )?;

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

    info!("Node {}: Prevote Quorum reached for round {}. Proceeding to send commits.", node_id, round_id);

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone(); // ✅ Clone the Arc<AtomicU64>

    // ✅ Enqueue own commit before broadcasting
    if let Some(rbc_processor) = &rbc_processor {
        info!("Node {}: Adding *local* commit to queue for round {}", node_id, round_id);
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request.clone())).await;
    } else {
        error!("Node {}: RBCProcessor not initialized when trying to enqueue *local* commit!", node_id);
    }


    for target_node in node_list {
        let target_url = format!("http://{}/commit", target_node);
        let commit_payload = commit_request.clone();
    
        let mut attempt = 0;
        let max_attempts = 3;
        let mut success = false;
    
        while attempt < max_attempts {
            attempt += 1;
    
            message_count.fetch_add(1, Ordering::Relaxed);
            info!(
                "📤 Attempt {}/{}: Node {} sending commit to {} for round {}",
                attempt, max_attempts, node_id, target_url, round_id
            );
    
            let res = client
                .post(&target_url)
                .json(&commit_payload)
                // .timeout(Duration::from_millis(500))  // Optional
                .send()
                .await;
    
            match res {
                Ok(response) if response.status().is_success() => {
                    info!("✅ Successfully sent commit to {}", target_url);
                    success = true;
                    break;
                }
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_else(|_| "No response".to_string());
                    error!("❌ Commit failed to {}. Status: {}. Body: {}", target_url, status, body);
                }
                Err(e) => {
                    error!("❌ Network error while sending commit to {}: {:?}", target_url, e);
                }
            }
    
            let delay = 100 * 2u64.pow((attempt - 1) as u32); // backoff
            sleep(Duration::from_millis(delay)).await;
        }
    
        if !success {
            error!(
                "❌ Node {}: Final failure to send commit to {} after {} attempts",
                node_id, target_url, max_attempts
            );
        }
    }
    
    Ok(())
}

