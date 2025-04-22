use std::sync::{atomic::Ordering, Arc};
use base64::{ engine::general_purpose, Engine };
use num_bigint::BigInt;
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{ error, info };
use crate::{
    processors::priority_queue::RBCMessage, structs::{ node::Node, requests::{ PrevoteRequest, ProposeRequest } }, utils::{dag_utils::ensure_dag_round_sync, rsa_accumulator_util::verify_proof}
};
use sha2::{Digest, Sha256};
use tokio::time::{sleep, Duration};
/*
**ch-RBC Proof Validation for `handle_propose`**
--------------------------------------------------

**Step 7:** Upon receiving `propose(h, b_j, s_j)` from `P_s`
   - This function (`handle_propose`) is invoked upon receiving a `ProposeRequest` from another node.

**Step 8:** If `received_propose(P_i, r)` then terminate
   - The proposal tracker is checked to ensure no duplicate proposals from the same sender (`P_s`) in the same round (`r`).
   - If a duplicate exists, the function returns early.

**Step 9:** If `check_size(s_j)` then
   - The function `check_size(&decoded_shards)` validates the shard sizes.
   - If the size is invalid, the function returns early.

**Step 20:** Wait until `D_i` reaches round `r−1`
   - The function `ensure_dag_round_sync(node.clone(), round_id).await?` ensures that the DAG is synchronized to `r-1` before proceeding.

**Step 11:** Multicast `prevote(h, b_j, s_j)` to all nodes
   - If enough proposals are received (`proposal_count >= required_proposals`), all `prevote` messages are aggregated into a single request and multicast to all nodes.

**Step 12:** `received_propose(P_i, r) = True`
   - The proposal is stored in `proposal_tracker` using `update_proposal_tracker()`, ensuring that the proposal is marked as received.

**Step 13:** Confirm proposal handling
   - Log that the proposal was successfully handled.
*/

// Step 7: Upon receiving `propose(h, b_j, s_j)` from `P_s`
// - This function `handle_propose` is called when a proposal message is received.

/*
**ch-RBC Proof Validation for `handle_propose`**
--------------------------------------------------
Handles proposals containing multiple transactions in a single request.
Each transaction has:
  - Merkle root
  - Proofs
  - Encoded shards
*/

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;
    let node_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;

        let proposal_tracker = node_guard.proposal_tracker.lock().await;
        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            if round_proposals.contains_key(&proposer_id) {
                info!("Node {}: Duplicate proposal from proposer {} for round {}. Ignoring.", node_id, proposer_id, round_id);
                return Ok(());
            }
        }
    }

    let accumulator_bytes = general_purpose::STANDARD
        .decode(&propose_request.batch_accumulator)
        .map_err(|e| format!("Node {}: Failed to decode accumulator: {:?}", node_id, e))?;

    let accumulator = BigInt::from_bytes_be(num_bigint::Sign::Plus, &accumulator_bytes);

    for (i, tx) in propose_request.transactions.iter().enumerate() {
        let shard_b64 = tx.shards.get(0)
            .ok_or_else(|| format!("Node {}: Missing shard for tx {}", node_id, i))?;
        let decoded_shard = general_purpose::STANDARD
            .decode(shard_b64)
            .map_err(|e| format!("Node {}: Failed to decode shard for tx {}: {:?}", node_id, i, e))?;
    
        let proof_b64 = tx.proofs.get(0)
            .ok_or_else(|| format!("Node {}: Missing proof for tx {}", node_id, i))?;
        let proof_bytes = general_purpose::STANDARD
            .decode(proof_b64)
            .map_err(|e| format!("Node {}: Failed to decode proof for tx {}: {:?}", node_id, i, e))?;
        let proof = BigInt::from_bytes_be(num_bigint::Sign::Plus, &proof_bytes);
    
        let hash = Sha256::digest(&decoded_shard);
    

        // info!(
        //     "Node {}: handle_propose() tx[{}] → decoded shard SHA256 = {}, proof base64 = {}...",
        //     node_id,
        //     i,
        //     hex::encode(&Sha256::digest(&decoded_shard)),
        //     &proof_b64[..8.min(proof_b64.len())],
        // );

        if !verify_proof(&accumulator, &hash, &proof) {
            return Err(format!("Node {}: RSA proof invalid for tx {}", node_id, i));
        }
    }

    ensure_dag_round_sync(node.clone(), round_id).await?;

    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    if proposal_count >= quorum_threshold {
        info!(
            "Node {}: Proposal quorum met. Broadcasting prevote for {} proposals.",
            node_id, stored_proposals.len()
        );

        let prevote_request = {
            let node_guard = node.lock().await;
            PrevoteRequest {
                proposals: stored_proposals.clone(),
                sender_url: node_guard.ip_address.clone(),
                sender_id: node_guard.id,
            }
        };

        let (node_ip, node_list) = {
            let node_guard = node.lock().await;
            (node_guard.ip_address.clone(), node_guard.nodes.clone())
        };

        for target_node in node_list {
            if target_node != node_ip {
                let url = format!("http://{}/prevote", target_node);
                for attempt in 1..=3 {
                    info!("📤 Attempt {}/3: Node {} → {}", attempt, node_id, url);
                    let res = client.post(&url).json(&prevote_request).send().await;

                    match res {
                        Ok(resp) if resp.status().is_success() => {
                            info!("✅ Node {}: Prevote delivered to {}", node_id, url);
                            break;
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let body = resp.text().await.unwrap_or_default();
                            error!("❌ Node {}: Prevote failed to {}: {} - {}", node_id, url, status, body);
                        }
                        Err(e) => error!("❌ Node {}: Network error to {}: {:?}", node_id, url, e),
                    }

                    sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
                }
                node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        if let Some(rbc_processor) = &node.lock().await.rbc_processor {
            info!("Node {}: Enqueuing local prevote", node_id);
            rbc_processor
                .enqueue_message(RBCMessage::Prevote(prevote_request))
                .await;
        } else {
            error!("Node {}: No RBCProcessor to enqueue local prevote", node_id);
        }
    }

    Ok(())
}





