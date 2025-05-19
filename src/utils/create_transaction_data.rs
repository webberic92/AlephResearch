use std::sync::Arc;
use tokio::sync::Mutex;
use sha2::{Digest, Sha256};
use tracing::{info, error};
use anyhow::Error;
use base64::{engine::general_purpose, Engine};
use reed_solomon_erasure::galois_8::ReedSolomon;

use crate::{
    structs::{node::Node, requests::{BaseRequest, ProposeRequest, Transaction}}, 
    utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, verify_merkle_proof}
};

/// Pads or truncates a vector to a target size
pub fn pad_to_len(mut data: Vec<u8>, target_len: usize) -> Vec<u8> {
    if data.len() >= target_len {
        data.truncate(target_len);
    } else {
        data.resize(target_len, 0);
    }
    data
}

pub async fn create_transaction_data(
    node: Arc<Mutex<Node>>, 
) -> Result<ProposeRequest, Error> {  
    let (node_id, num_txs, data_shards, total_nodes, transaction_size, round_id, parent_units) = {
        let node_guard = node.lock().await;
        let round_id = *node_guard.current_round.lock().await;
    
        let parent_units = node_guard.get_all_parents(round_id).await;
        info!("Node {}: Getting parent units for round {}: {:?}", node_guard.id, round_id, parent_units);

    
        info!(
            "Node {}: For round {}, resolved {} parent units: {:?}",
            node_guard.id,
            round_id,
            parent_units.len(),
            parent_units
        );
    
        (
            node_guard.id,
            node_guard.number_of_transactions,
            node_guard.data_shards,
            node_guard.total_nodes,
            node_guard.transaction_size,
            round_id,
            parent_units,
        )
    };

    let shard_size = (transaction_size + data_shards - 1) / data_shards; // ceil division
    info!(
        "Node {}: Creating {} transactions ({} bytes each) with RS shards ({} bytes/shard)",
        node_id, num_txs, transaction_size, shard_size
    );

    let mut tx_hashes = Vec::new();
    let mut transactions = Vec::new();

    for tx_index in 0..num_txs {
        let content = format!("tx{}_round{}", tx_index + 1, round_id);
        let mut padded = content.into_bytes();
        padded.resize(transaction_size, 0);

        let tx_hash = Sha256::digest(&padded).to_vec();
        tx_hashes.push(tx_hash.clone());

        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards)
            .map_err(|e| Error::msg(format!("RS init failed: {:?}", e)))?;

        // ✅ Manual split to ensure all shards are shard_size
        let mut data_chunks: Vec<Vec<u8>> = Vec::with_capacity(data_shards);
        for i in 0..data_shards {
            let start = i * shard_size;
            let end = std::cmp::min(start + shard_size, padded.len());
            let mut chunk = padded[start..end].to_vec();
            chunk.resize(shard_size, 0);
            data_chunks.push(chunk);
        }

        // Pad to total_nodes
        let mut shards = data_chunks.clone();
        while shards.len() < total_nodes {
            shards.push(vec![0u8; shard_size]);
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        rs.encode(&mut shard_refs)
            .map_err(|e| Error::msg(format!("RS encoding failed: {:?}", e)))?;

        let encoded_shards: Vec<String> = shards
            .into_iter()
            .map(|shard| general_purpose::STANDARD.encode(&shard))
            .collect();

        // info!(
        //     "Node {}: TX[{}] {:?}, hash = {}",
        //     node_id, tx_index, padded, hex::encode(&tx_hash)
        // );

        transactions.push(Transaction {
            root: tx_hash,
            proofs: vec![],
            shards: encoded_shards,
        });
    }

    let batch_root = compute_merkle_root(&tx_hashes);
    let batch_proofs: Vec<Vec<Vec<u8>>> = tx_hashes
        .iter()
        .enumerate()
        .map(|(i, _)| compute_merkle_branch(&tx_hashes, i))
        .collect();

    for (i, hash) in tx_hashes.iter().enumerate() {
        let proof = &batch_proofs[i];
        if !verify_merkle_proof(hash, proof, &batch_root, i) {
            error!("❌ Merkle proof verification failed for tx[{}]", i);
            return Err(Error::msg(format!("Merkle proof failed at tx[{}]", i)));
        }
    }

    info!(
        "✅ Proposal ready: {} txs, Round {}, Batch Root: {}",
        num_txs,
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

