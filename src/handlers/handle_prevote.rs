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
        requests::{CommitRequest, DagUnit, PrevoteRequest, ProposeRequest, Transaction},
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


// **Handles an incoming PREVOTE request with multiple proposals**
pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,  
) -> Result<(), String> {
    info!(
        "🔹 handle_prevote: Processing {} proposals from {}",
        prevote_request.proposals.len(),
        prevote_request.sender_url
    );

    let node_id;
    let round_id;
    let quorum_threshold;
    let total_nodes;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        round_id = prevote_request.proposals[0].base.round_id;
        quorum_threshold = node_guard.get_quorum_threshold();
        total_nodes = node_guard.total_nodes;
    }

    let mut reconstructed_units = Vec::new();

    // ✅ **Process each proposal separately**
    for proposal in &prevote_request.proposals {
        info!("🔹 Processing proposal from Node {} for round {}", proposal.base.proposing_node_id, round_id);

        let mut reconstructed_transactions = Vec::new();

        for transaction in &proposal.transactions {
            info!("🔹 Processing transaction with Merkle root: {:?}", transaction.root);

            // **Step 14:** Decode shards
            let decoded_shards: Vec<Vec<u8>> = transaction.shards
                .iter()
                .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
                .collect::<Result<Vec<Vec<u8>>, _>>()
                .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;
            
            info!("🔹 handle prevote Decoded shards: {:?}", decoded_shards);

            // Compute hash of each decoded shard
            let shard_hashes: Vec<Vec<u8>> = decoded_shards
                .iter()
                .map(|shard| Sha256::digest(shard).to_vec())
                .collect();
            info!("🔹 handle prevote shard_hashes: {:?}", shard_hashes);



            // **Step 15:** Validate Merkle proofs for each shard
            for (shard_index, shard) in decoded_shards.iter().enumerate() {
                let decoded_proof: Vec<Vec<u8>> = transaction.proofs[shard_index]
                    .iter()
                    .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
                    .collect::<Result<Vec<Vec<u8>>, _>>()
                    .map_err(|e| format!("Node {}: Failed to decode proof: {:?}", node_id, e))?;
            
                info!(
                    "🔹 handle prevote: Validating Merkle proof for shard {} with proof: {:?}",
                    shard_index, decoded_proof
                );
            
                if !validate_merkle_branch(&shard_hashes[shard_index], &decoded_proof, shard_index, &transaction.root) {
                    return Err(format!(
                        "Node {}: Merkle root mismatch for shard {}",
                        node_id, shard_index
                    ));
                }
            }
            

            // **Step 18:** Interpolate missing shares (if needed)
            info!("========TEST==============");
            let interpolated_shards;
            if decoded_shards.len() < total_nodes {
                info!("🔹TEST A decoded shards length < totalnodes with Merkle root: {:?}", transaction.root);
             interpolated_shards =  interpolate_shares(&decoded_shards, round_id)
                    .map_err(|e| format!("Node {}: Failed to interpolate shares: {:?}", node_id, e))?
            } else {
                info!("🔹TEST B Interpolating shares for transaction with Merkle root: {:?}", transaction.root);

                interpolated_shards= decoded_shards.clone()
            };

            // ✅ Compute SHA-256 hashes before computing the Merkle root
            let interpolated_shard_hashes: Vec<Vec<u8>> = interpolated_shards
                .iter()
                .map(|shard| Sha256::digest(shard).to_vec())
                .collect();
            info!("🔹 TEST C handle prevote interpolated_shard_hashes: {:?}", interpolated_shard_hashes);

            // **Step 19:** Compute Merkle root from hashed interpolated shares
            let new_merkle_root = compute_merkle_root(&interpolated_shard_hashes);

            info!("🔹 handle prevote interpolated_shards: {:?}", interpolated_shard_hashes);


            if new_merkle_root != transaction.root {
                return Err(format!(
                    "Node {}: Merkle root mismatch after interpolation. Expected {:?} but got {:?}",
                    node_id, transaction.root, new_merkle_root
                ));
            }

            // ✅ **Reconstruct transaction with decoded shards**
            let reconstructed_tx = Transaction {
                root: transaction.root.clone(),
                proofs: transaction.proofs.clone(),
                shards: interpolated_shards.iter().map(|s| String::from_utf8_lossy(s).to_string()).collect(), 
            };

            reconstructed_transactions.push(reconstructed_tx);
        }

        // ✅ **Step 15: Reconstruct the entire unit**
        let reconstructed_unit = reconstruct_unit(
            &reconstructed_transactions,  // ✅ Pass **transactions** instead of raw shards
            round_id,
            proposal.parents.clone(),
            proposal.base.proposing_node_id as usize,
        )
        .map_err(|e| format!("Node {}: Reconstruction failed: {:?}", node_id, e))?;

        if reconstructed_unit.transactions.is_empty() {
            return Err(format!("Node {}: Reconstructed unit is invalid or empty", node_id));
        }

        reconstructed_units.push(reconstructed_unit);
    }

    // **Step 14 (continued):** Count quorum votes ONCE per PrevoteRequest (not per proposal)
    let epoch_key = round_id.to_be_bytes().to_vec();
    let vote_count = {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let count = quorum_votes.entry(epoch_key.clone()).or_insert(0);
        *count += 1;
        *count
    };

    info!(
        "🔹 Node {}: Quorum votes {}/{}",
        node_id, vote_count, quorum_threshold
    );

    if vote_count < quorum_threshold {
        info!(
            "🔹 Node {}: Not enough prevote messages received. Waiting for quorum before proceeding to commit.",
            node_id
        );
        return Ok(());
    }

    // **Step 20:** Prepare commit request
    let commit_request = CommitRequest {
        units: reconstructed_units,  // ✅ Use all reconstructed units
        proposing_node_id: node_id,
        round_id,
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
    handle_commit(node.clone(), commit_request)
        .await
        .map_err(|e| format!("Commit phase failed: {:?}", e))?;

    info!("✅ Node {}: Successfully processed PREVOTE for round {}.", node_id, round_id);

    Ok(())
}




//WORKS
// pub async fn handle_prevote(
//     node: Arc<Mutex<Node>>,
//     client: Arc<Client>,
//     prevote_request: PrevoteRequest,  
// ) -> Result<(), String> {
//     let node_id;
//     let round_id;

//     {
//         let node_guard = node.lock().await;
//         node_id = node_guard.id;
//         round_id = prevote_request.proposals[0].base.round_id;
//     }

//     for proposal in &prevote_request.proposals {
//         info!(
//             "🔹 handle_prevote: Processing proposal from Node {} for round {}",
//             proposal.base.proposing_node_id, round_id
//         );

//         for transaction in &proposal.transactions {
//             info!(
//                 "🔹 handle_prevote: Processing transaction with received Merkle root: {:?}",
//                 transaction.root
//             );

//             // **Step 1: Decode the received shards**
//             let decoded_shards: Vec<Vec<u8>> = transaction.shards
//                 .iter()
//                 .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
//                 .collect::<Result<Vec<Vec<u8>>, _>>()
//                 .map_err(|e| format!("Node {}: Failed to decode shards: {:?}", node_id, e))?;

//             info!("🔹 handle_prevote: Decoded shards: {:?}", decoded_shards);

//             // **Step 2: Compute the SHA-256 hash of each shard**
//             let shard_hashes: Vec<Vec<u8>> = decoded_shards
//                 .iter()
//                 .map(|shard| Sha256::digest(shard).to_vec())
//                 .collect();

//             info!("🔹 handle_prevote: Computed shard hashes: {:?}", shard_hashes);

//             // **Step 3: Compute Merkle root from the decoded shards**
//             let computed_merkle_root = compute_merkle_root(&shard_hashes);

//             info!(
//                 "🔹 handle_prevote: Computed Merkle root: {:?}, Expected Merkle root: {:?}",
//                 computed_merkle_root, transaction.root
//             );

//             // **Step 4: Validate the Merkle root**
//             if computed_merkle_root != transaction.root {
//                 return Err(format!(
//                     "Node {}: Merkle root mismatch at Prevote! Expected {:?}, but computed {:?}",
//                     node_id, transaction.root, computed_merkle_root
//                 ));
//             }

//             info!("✅ Node {}: Merkle root verification PASSED at Prevote!", node_id);
//         }
//     }

//     Ok(())
// }
