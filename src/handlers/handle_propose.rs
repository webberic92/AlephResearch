use std::collections::HashMap;
use std::sync::{atomic::Ordering, Arc};

use base64::{engine::general_purpose, Engine};
use num_bigint::{BigInt, Sign};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use tracing::info;

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{
        node::Node,
        requests::{PrevoteRequest, ProposeRequest},
    },
    utils::{
        dag_utils::ensure_dag_round_sync,
        rsa_accumulator_util::{compute_accumulator_from_primes, memoized_hash_to_prime},
    },
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;

    let node_id = {
        let node_guard = node.lock().await;
        node_guard.id
    };

    // Always sync DAG before proposal processing
    ensure_dag_round_sync(node.clone(), round_id).await?;
    let duration_dag_sync = timer_total.elapsed();

    // Insert proposal into tracker
    {
        let node_guard = node.lock().await;

        // Duplicate check
        let mut tracker = node_guard.proposal_tracker.lock().await;
        let entry = tracker.entry(round_id).or_insert_with(HashMap::new);
        if entry.contains_key(&proposer_id) {
            info!("Node {}: Duplicate proposal {} for round {}", node_id, proposer_id, round_id);
            return Ok(());
        }

        entry.insert(proposer_id, propose_request.clone());

        let proposal_count = entry.len();
        let quorum_threshold = node_guard.total_nodes - node_guard.get_fault_tolerance_threshold();
        info!("Node {}: Added proposal for round {} from node {}. Count: {}/{}", node_id, round_id, proposer_id, proposal_count, quorum_threshold);

        if proposal_count >= quorum_threshold {
            if proposal_count > quorum_threshold {
                info!("Node {}: Quorum already reached for round {}, ignoring extra proposal from node {}.", node_id, round_id, proposer_id);
            }
            return Ok(());
        }
    }

    // ✅ Quorum threshold reached, now verify before locking
    let mut stored_proposals = {
        let node_guard = node.lock().await;
        let tracker = node_guard.proposal_tracker.lock().await;
        tracker.get(&round_id).unwrap().values().cloned().collect::<Vec<_>>()
    };

    // Canonical proposal ordering
    stored_proposals.sort_by_key(|p| (p.base.proposing_node_id, p.base.round_id));

    let mut all_shard_hashes: Vec<String> = Vec::new();
    for prop in &stored_proposals {
        for tx in &prop.transactions {
            for hash_hex in &tx.shard_hashes {
                all_shard_hashes.push(hash_hex.clone());
            }
        }
    }

    all_shard_hashes.sort_unstable();
    all_shard_hashes.dedup();

    let mut all_primes: Vec<BigInt> = Vec::new();
    for hash_hex in &all_shard_hashes {
        let prime = memoized_hash_to_prime(hash_hex).await;
        all_primes.push(prime);
    }

    let expected_batch_acc = compute_accumulator_from_primes(&all_primes);
    let received_batch_bytes = general_purpose::STANDARD
        .decode(&propose_request.batch_accumulator)
        .map_err(|e| format!("Batch accumulator decode error: {:?}", e))?;
    let received_batch_acc = BigInt::from_bytes_be(Sign::Plus, &received_batch_bytes);

    if expected_batch_acc != received_batch_acc {
        return Err(format!("Batch accumulator mismatch: computed={:?} received={:?}", expected_batch_acc, received_batch_acc));
    }

    // ✅ Only now lock after verification
    {
        let node_guard = node.lock().await;
        let mut locks = node_guard.proposal_locks.lock().await;
        if locks.contains(&round_id) {
            info!("Node {}: Proposal set already locked after verification for round {}", node_id, round_id);
            return Ok(());
        }
        locks.insert(round_id);
        info!("Node {}: Proposal set verified and locked for round {}", node_id, round_id);
    }

    // Build proposal_digest deterministically
    let mut proposal_hashes: Vec<String> = stored_proposals.iter()
        .map(|p| {
            let id_bytes = format!("{}-{}", p.base.proposing_node_id, p.base.round_id).into_bytes();
            hex::encode(Sha256::digest(&id_bytes))
        })
        .collect();
    proposal_hashes.sort_unstable();
    let combined_input: Vec<u8> = proposal_hashes.concat().into_bytes();
    let proposal_digest = hex::encode(Sha256::digest(&combined_input));

    info!("Node {}: Proposal digest locked as {}", node_id, proposal_digest);

    let prevote_request = {
        let node_guard = node.lock().await;
        PrevoteRequest {
            proposals: stored_proposals.clone(),
            sender_url: node_guard.ip_address.clone(),
            sender_id: node_guard.id,
            batch_accumulator: propose_request.batch_accumulator.clone(),
            proposal_digest: proposal_digest.clone(),
        }
    };

    let (node_ip, node_list) = {
        let node_guard = node.lock().await;
        (node_guard.ip_address.clone(), node_guard.nodes.clone())
    };

    let client = Client::new();
    for peer in node_list {
        if peer != node_ip {
            let url = format!("http://{}/prevote", peer);
            for attempt in 1..=3 {
                if let Ok(resp) = client.post(&url).json(&prevote_request).send().await {
                    if resp.status().is_success() {
                        break;
                    }
                }
                sleep(Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32))).await;
            }
            node.lock().await.message_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    if let Some(rbc_processor) = &node.lock().await.rbc_processor {
        rbc_processor.enqueue_message(RBCMessage::Prevote(prevote_request)).await;
    }

    info!("Node {}: ✅ handle_propose round {} done in {:?} [dag_sync: {:?}]", node_id, round_id, timer_total.elapsed(), duration_dag_sync);
    Ok(())
}
