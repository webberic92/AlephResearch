use std::sync::Arc;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::{info, error};
use anyhow::Error;
use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}}, 
    utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, verify_merkle_proof}
};
use base64::{engine::general_purpose, Engine};

/// Pads or truncates a vector to exactly 250 bytes
pub fn pad_to_250(mut data: Vec<u8>) -> Vec<u8> {
    if data.len() >= 250 {
        data.truncate(250);
    } else {
        data.resize(250, 0);
    }
    data
}

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<ProposeRequest, Error> {  
    let (node_id, node_number_of_transactions);
    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
        node_number_of_transactions = node_guard.number_of_transactions;
    }

    info!(
        "Node {}: Creating {} transactions (each 250 bytes) with a single Merkle tree",
        node_id, node_number_of_transactions
    );

    let mut padded_tx_data = Vec::new();      // ✅ Store actual padded content (for debugging if needed)
    let mut tx_hashes = Vec::new();
    let mut transactions = Vec::new();

    for tx_index in 0..node_number_of_transactions {
        let content = format!("node{}_tx{}", node_id, tx_index);
        let padded = pad_to_250(content.clone().into_bytes());
        let tx_hash = Sha256::digest(&padded).to_vec();
        let encoded = general_purpose::STANDARD.encode(&padded);

        padded_tx_data.push(padded.clone());
        tx_hashes.push(tx_hash.clone());

        transactions.push(Transaction {
            root: tx_hash,
            proofs: vec![],           // ⛔ These are not used anymore, see batch_proofs below
            shards: vec![encoded],
        });

        info!("🔐 tx[{}] hash = {:x?}", tx_index, Sha256::digest(&padded));
        info!("📦 tx[{}] padded (first 8): {:?}", tx_index, &padded[..8]);
    }

    let batch_root = compute_merkle_root(&tx_hashes);
    let batch_proofs: Vec<Vec<Vec<u8>>> = tx_hashes
        .iter()
        .enumerate()
        .map(|(i, _)| compute_merkle_branch(&tx_hashes, i))
        .collect();

    // ✅ Internal proof check before sending
    for (i, hash) in tx_hashes.iter().enumerate() {
        let proof = &batch_proofs[i];
        let valid = verify_merkle_proof(hash, proof, &batch_root, i);
        if !valid {
            error!("❌ Merkle proof verification failed for tx[{}]", i);
            error!("  leaf hash: {}", hex::encode(hash));
            error!("  proof: {:?}", batch_proofs[i].iter().map(hex::encode).collect::<Vec<_>>());
            error!("  expected root: {}", hex::encode(&batch_root));
            let recomputed = crate::utils::merkle_utils::validate_merkle_branch(hash, &batch_proofs[i], i, &batch_root);
            error!("  alt validate_merkle_branch result = {}", recomputed);
            panic!("Merkle proof mismatch at tx[{}]", i);
        }
    }

    let (round_id, parent_units);
    {
        let node_guard = node.lock().await;
        round_id = *node_guard.current_round.lock().await;
        parent_units = node_guard
            .get_all_parents(round_id)
            .await
            .into_iter()
            .map(|s| s.into_bytes())
            .collect();
    }

    info!(
        "✅ Proposal ready: {} txs, Round {}, Batch Root: {}",
        node_number_of_transactions,
        round_id,
        hex::encode(&batch_root)
    );

    Ok(ProposeRequest {
        base: BaseRequest {
            proposing_node_id: node_id as u8,
            round_id,
        },
        transactions,
        parents: parent_units,
        batch_root,
        batch_proofs,
    })
}
