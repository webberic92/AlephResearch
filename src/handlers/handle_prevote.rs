use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    sync::Mutex,
    task::spawn_blocking,
    time::{sleep, Duration, Instant},
};
use tracing::info;
use num_bigint::{BigInt, Sign};

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{
        node::Node,
        requests::{CommitRequest, DagUnit, PrevoteRequest},
    },
    utils::rsa_accumulator_util::{compute_accumulator_from_primes, memoized_hash_to_prime},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();

    let (node_id, round_id, quorum_threshold, node_list, rbc_processor) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
        )
    };

    let mut proposals = prevote_request.proposals.clone();
    proposals.sort_by_key(|p| (p.base.proposing_node_id, p.base.round_id));

    let mut proposal_hashes: Vec<String> = proposals
        .iter()
        .map(|p| {
            let id_bytes = format!("{}-{}", p.base.proposing_node_id, p.base.round_id).into_bytes();
            hex::encode(Sha256::digest(&id_bytes))
        })
        .collect();
    proposal_hashes.sort_unstable();
    let combined_input: Vec<u8> = proposal_hashes.concat().into_bytes();
    let local_digest = hex::encode(Sha256::digest(&combined_input));

    info!("Node {}: Computed local proposal_digest: {}", node_id, local_digest);

    if local_digest != prevote_request.proposal_digest {
        return Err(format!(
            "❌ Proposal digest mismatch: received {}, computed {}",
            prevote_request.proposal_digest, local_digest
        ));
    }

    let reached_quorum_now: bool;
    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let round_key = round_id.to_be_bytes().to_vec();
        let voters = quorum_votes.entry(round_key.clone()).or_insert_with(HashSet::new);

        if !voters.insert(prevote_request.sender_url.clone()) {
            info!("Node {}: Duplicate prevote from {} for round {}.", node_id, prevote_request.sender_url, round_id);
            return Ok(());
        }

        let current_votes = voters.len();
        if current_votes < quorum_threshold {
            info!("Node {}: Waiting for more prevotes ({}/{})", node_id, current_votes, quorum_threshold);
            return Ok(());
        }

        if current_votes > quorum_threshold {
            info!("Node {}: Quorum already reached for round {}, skipping duplicate processing.", node_id, round_id);
            return Ok(());
        }

        reached_quorum_now = true;
    }

    if reached_quorum_now {
        info!("Node {}: ✅ Quorum reached for round {}. Verifying batch accumulator...", node_id, round_id);

        let mut all_shard_hashes: Vec<String> = Vec::new();
        for proposal in &proposals {
            for tx in &proposal.transactions {
                for hash_hex in &tx.shard_hashes {
                    all_shard_hashes.push(hash_hex.clone());
                }
            }
        }

        all_shard_hashes.sort_unstable();
        all_shard_hashes.dedup();

        let prime_tasks = all_shard_hashes.iter().map(|hash_hex| {
            let hash_hex = hash_hex.clone();
            spawn_blocking(move || futures::executor::block_on(memoized_hash_to_prime(&hash_hex)))
        });
        let prime_results = futures::future::join_all(prime_tasks).await;

        let mut all_primes = Vec::with_capacity(prime_results.len());
        for res in prime_results {
            match res {
                Ok(prime) => all_primes.push(prime),
                Err(_) => return Err("spawn_blocking failed on hash_to_prime".into()),
            }
        }

        let computed_batch_acc = compute_accumulator_from_primes(&all_primes);

        let received_batch_bytes = general_purpose::STANDARD
            .decode(&prevote_request.batch_accumulator)
            .map_err(|e| format!("Batch accumulator decode failed: {:?}", e))?;
        let received_batch_acc = BigInt::from_bytes_be(Sign::Plus, &received_batch_bytes);

        if computed_batch_acc != received_batch_acc {
            return Err(format!(
                "Batch accumulator mismatch: local_computed_acc={:?} received_acc={:?}",
                computed_batch_acc, received_batch_acc
            ));
        }

        info!("Node {}: ✅ Batch accumulator verified. Proceeding to DAG commit...", node_id);
    }

    let mut reconstructed_units = Vec::new();
    for proposal in &proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;

        reconstructed_units.push(DagUnit {
            unit_id: format!("U{}-{}", round_id, proposer_id),
            proposer_node: proposer_id,
            round: round_id,
            transactions: proposal.transactions.clone(),
            parent_units: proposal.parents.iter().map(|p| hex::encode(p)).collect(),
            accumulator_root: vec![],
            finalization_timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    let commit_request = CommitRequest {
        units: reconstructed_units,
        proposing_node_id: node_id,
        round_id,
    };

    let message_count = node.lock().await.message_count.clone();
    let client = Client::new();

    for peer in node_list {
        let url = format!("http://{}/commit", peer);
        for attempt in 0..3 {
            message_count.fetch_add(1, Ordering::Relaxed);
            match client.post(&url).json(&commit_request).send().await {
                Ok(resp) if resp.status().is_success() => break,
                _ => sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await,
            }
        }
    }

    if let Some(rbc_processor) = rbc_processor {
        rbc_processor.enqueue_message(RBCMessage::Commit(commit_request)).await;
    }

    info!("Node {}: ✅ handle_prevote round {} done in {:?}", node_id, round_id, timer_total.elapsed());
    Ok(())
}
