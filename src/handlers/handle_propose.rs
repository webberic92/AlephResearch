use std::sync::{atomic::Ordering, Arc};

use base64::{engine::general_purpose, Engine};
use futures::future::join_all;
use num_bigint::{BigInt, Sign};
use reqwest::Client;
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
        rsa_accumulator_util::{compute_accumulator_from_primes, get_modulus, memoized_hash_to_prime},
    },
};

pub async fn handle_propose(
    node: Arc<Mutex<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let mut duration_dag_sync = Duration::ZERO;
    let round_id = propose_request.base.round_id;
    let proposer_id = propose_request.base.proposing_node_id as usize;
    let node_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        let proposal_tracker = node_guard.proposal_tracker.lock().await;
        if let Some(round_proposals) = proposal_tracker.get(&round_id) {
            if round_proposals.contains_key(&proposer_id) {
                info!(
                    "Node {}: Duplicate proposal {} for round {}",
                    node_id, proposer_id, round_id
                );
                return Ok(());
            }
        }
    }

    ensure_dag_round_sync(node.clone(), round_id).await?;
    duration_dag_sync = timer_total.elapsed();

    let (proposal_count, quorum_threshold, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

    if proposal_count < quorum_threshold {
        info!(
            "Node {}: Waiting for quorum ({} < {})",
            node_id, proposal_count, quorum_threshold
        );
        return Ok(());
    }

    info!(
        "Node {}: Quorum reached for round {}, verifying proofs and broadcasting prevote...",
        node_id, round_id
    );

    let modulus = get_modulus();

    // ⛓️ Collect verification tasks for all data shard proofs
    let mut verification_tasks = Vec::new();

    for tx in &propose_request.transactions {
        let shard_hashes = tx
            .shard_hashes
            .as_ref()
            .ok_or("Missing shard_hashes field in transaction".to_string())?;

        let acc_encoded = tx
            .accumulator
            .as_ref()
            .ok_or("Missing accumulator in transaction".to_string())?;
        let acc_bytes = general_purpose::STANDARD
            .decode(acc_encoded)
            .map_err(|e| format!("Accumulator decode failed: {:?}", e))?;
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

        for (j, shard) in tx.shards.iter().enumerate() {
            if j >= tx.number_of_data_shards || j >= shard_hashes.len() || shard.proofs.is_empty() {
                continue;
            }

            let proof_b64 = &shard.proofs[0];
            let proof_bytes = general_purpose::STANDARD
                .decode(proof_b64)
                .map_err(|e| format!("Proof decode error: {:?}", e))?;
            let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
            let hash_hex = shard_hashes[j].clone();
            let accumulator = accumulator.clone();
            let modulus = modulus.clone();

            verification_tasks.push(async move {
                let prime = memoized_hash_to_prime(&hash_hex).await;
                let valid = proof.modpow(&prime, &modulus) == accumulator;
                if valid {
                    Ok(prime)
                } else {
                    Err(format!("❌ RSA proof invalid for shard hash {}", hash_hex))
                }
            });
        }
    }

    // 🧠 Run all async proof validations
    let results: Vec<Result<BigInt, String>> = join_all(verification_tasks).await;
    let mut primes: Vec<BigInt> = results.into_iter().collect::<Result<_, _>>()?;

    // 🔁 Sort and deduplicate primes
    primes.sort();
    primes.dedup();

    // ✅ Check batch accumulator
    let expected_batch_acc = compute_accumulator_from_primes(&primes);
    let received_batch_bytes = general_purpose::STANDARD
        .decode(&propose_request.batch_accumulator)
        .map_err(|e| format!("Batch accumulator decode error: {:?}", e))?;
    let received_batch_acc = BigInt::from_bytes_be(Sign::Plus, &received_batch_bytes);

    if expected_batch_acc != received_batch_acc {
        return Err("❌ Batch accumulator mismatch".to_string());
    }

    // 📤 Prepare Prevote
    let prevote_request = {
        let node_guard = node.lock().await;
        PrevoteRequest {
            proposals: stored_proposals.clone(),
            sender_url: node_guard.ip_address.clone(),
            sender_id: node_guard.id,
            batch_accumulator: propose_request.batch_accumulator.clone(),
        }
    };

    // 🌍 Broadcast to peers
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
            node.lock()
                .await
                .message_count
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    // 🧠 Push to local queue
    if let Some(rbc_processor) = &node.lock().await.rbc_processor {
        rbc_processor
            .enqueue_message(RBCMessage::Prevote(prevote_request))
            .await;
    }

    info!(
        "Node {}: ✅ handle_propose round {} done in {:?} [dag_sync: {:?}]",
        node_id,
        round_id,
        timer_total.elapsed(),
        duration_dag_sync
    );

    Ok(())
}
