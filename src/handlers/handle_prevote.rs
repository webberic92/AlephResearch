use base64::{engine::general_purpose, Engine};
use futures::stream::{FuturesUnordered, StreamExt};
use rayon::prelude::*;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::{
    sync::Mutex,
    time::{sleep, Duration, Instant},
};
use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Arc},
};
use tracing::{error, info};
use num_bigint::{BigInt, Sign};

use crate::{
    processors::priority_queue::RBCMessage,
    structs::{
        node::Node,
        requests::{CommitRequest, DagUnit, PrevoteRequest},
    },
    utils::rsa_accumulator_util::{compute_accumulator_from_primes, get_modulus, memoized_hash_to_prime},
};

pub async fn handle_prevote(
    node: Arc<Mutex<Node>>,
    prevote_request: PrevoteRequest,
) -> Result<(), String> {
    let timer_total = Instant::now();
    let modulus = get_modulus();

    let (node_id, round_id, quorum_threshold, node_list, rbc_processor, transaction_size) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (
            node_guard.id,
            prevote_request.proposals[0].base.round_id,
            node_guard.get_quorum_threshold(),
            node_guard.nodes.clone(),
            node_guard.rbc_processor.clone(),
            node_guard.transaction_size,
        )
    };

    {
        let node_guard = node.lock().await;
        let mut quorum_votes = node_guard.quorum_votes.lock().await;
        let voters = quorum_votes
            .entry(round_id.to_be_bytes().to_vec())
            .or_insert_with(HashSet::new);
        if !voters.insert(prevote_request.sender_url.clone()) {
            info!(
                "Node {}: Duplicate prevote from {} for round {}.",
                node_id, prevote_request.sender_url, round_id
            );
            return Ok(());
        }
        if voters.len() < quorum_threshold {
            info!(
                "Node {}: Waiting for more prevotes ({}/{})",
                node_id,
                voters.len(),
                quorum_threshold
            );
            return Ok(());
        }
    }

    info!(
        "Node {}: Quorum reached for round {}. Verifying proofs...",
        node_id, round_id
    );

    // Collect all hash_hex, proof, accumulator tuples
    let mut all_proofs = Vec::new();

    for proposal in &prevote_request.proposals {
        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let acc_encoded = tx.accumulator.as_ref().ok_or("Missing accumulator")?;
            let acc_bytes = general_purpose::STANDARD
                .decode(acc_encoded)
                .map_err(|e| format!("Accumulator decode error: {:?}", e))?;
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

            let data_shards = tx.number_of_data_shards;
            if tx.shards.len() < data_shards {
                return Err(format!(
                    "tx[{}]: not enough shards ({} < {})",
                    tx_index, tx.shards.len(), data_shards
                ));
            }

            for shard_index in 0..data_shards {
                let shard = &tx.shards[shard_index];

                let expected_hash_hex = tx.shard_hashes
                    .as_ref()
                    .and_then(|h| h.get(shard_index))
                    .ok_or_else(|| format!("Missing hash for tx[{}] shard[{}]", tx_index, shard_index))?
                    .clone();

                let proof_b64 = shard.proofs.get(0).ok_or("Missing proof")?;
                let proof_bytes = general_purpose::STANDARD
                    .decode(proof_b64)
                    .map_err(|e| format!("Proof decode error: {:?}", e))?;
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

                all_proofs.push((expected_hash_hex, proof, accumulator.clone()));
            }
        }
    }

    // Fully async prime mapping and RSA proof check
    let mut verification_tasks = FuturesUnordered::new();

    for (hash_hex, proof, accumulator) in all_proofs {
        let modulus = modulus.clone();
        verification_tasks.push(async move {
            let prime = memoized_hash_to_prime(&hash_hex).await;
            let valid = proof.modpow(&prime, &modulus) == accumulator;
            if valid {
                Ok(prime)
            } else {
                Err(format!("❌ Invalid RSA proof for hash {}", hash_hex))
            }
        });
    }

    let mut primes = Vec::new();
    while let Some(result) = verification_tasks.next().await {
        match result {
            Ok(prime) => primes.push(prime),
            Err(e) => return Err(e),
        }
    }

    // ✅ Batch accumulator check
    let expected_bytes = general_purpose::STANDARD
        .decode(&prevote_request.batch_accumulator)
        .map_err(|e| format!("Failed to decode batch accumulator: {:?}", e))?;
    let expected_acc = BigInt::from_bytes_be(Sign::Plus, &expected_bytes);
    let computed_acc = compute_accumulator_from_primes(&primes);
    if computed_acc != expected_acc {
        return Err("❌ Batch accumulator mismatch".to_string());
    }

    info!(
        "Node {}: Proofs validated. Reconstructing transactions...",
        node_id
    );

    // Reconstruct all valid transactions
    let mut reconstructed_units = Vec::new();
    for proposal in &prevote_request.proposals {
        let proposer_id = proposal.base.proposing_node_id as usize;
        let mut transactions = Vec::new();

        for tx in &proposal.transactions {
            let mut buffer = Vec::with_capacity(transaction_size);
            for shard in tx.shards.iter().take(tx.number_of_data_shards) {
                let decoded = general_purpose::STANDARD
                    .decode(&shard.shard_b64)
                    .map_err(|e| format!("Shard decode error: {:?}", e))?;
                buffer.extend_from_slice(&decoded);
            }
            buffer.truncate(transaction_size);
            let hash = Sha256::digest(&buffer);
            if hash.to_vec() != tx.root {
                return Err("❌ Transaction hash mismatch".to_string());
            }
            transactions.push(tx.clone());
        }

        reconstructed_units.push(DagUnit {
            unit_id: format!("U{}-{}", round_id, proposer_id),
            proposer_node: proposer_id,
            round: round_id,
            transactions,
            parent_units: proposal
                .parents
                .iter()
                .map(|p| String::from_utf8_lossy(p).to_string())
                .collect(),
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

    info!(
        "Node {}: ✅ handle_prevote round {} done in {:?}",
        node_id,
        round_id,
        timer_total.elapsed()
    );

    Ok(())
}
