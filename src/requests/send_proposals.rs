use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    handlers::handle_propose::handle_propose,
    structs::{ node::Node, requests::{ BaseRequest, ProposeRequest } },
    utils::merkle_utils::{ compute_merkle_branch, compute_merkle_root },
};

/* 
**ch-RBC Proof Validation for `send_proposals`**
--------------------------------------------------

1. If `P_i = P_s`, then:
   - This function is called when a node needs to broadcast its proposal.

2. `{s_j} j∈N ← shares of (f + 1, N)-erasure coding of U`:
   - The `shards` parameter represents the erasure-coded shares.

3. `h ← Merkle tree root of {s_j} j∈N`:
   - The function `compute_merkle_root` is used to compute the Merkle root.

4. For each node `j ∈ N`:
   - A loop iterates over all nodes in `node_read.nodes` to send proposals.

5. `b_i ← Merkle branch of s_j`:
   - The function `compute_merkle_branch` computes the Merkle branch.

6. `send propose(h, b_j, s_j) to P_j`:
   - The proposals are sent asynchronously using `reqwest::Client::post`.
*/

pub async fn send_proposals(
    client: Arc<Client>,  
    node: Arc<Mutex<Node>>,
    shards: &[Vec<u8>],  // ✅ Raw binary shards (not encoded)
    merkle_roots: &[Vec<u8>], // ✅ Multiple Merkle roots (one per transaction)
    parent_units: Vec<String>,
) -> Result<(), anyhow::Error> {
    // ✅ **Step 1: Identify if P_i = P_s**
    let round;
    let node_id;
    let nodes;

    {
        let node_guard = node.lock().await;
        round = *node_guard.current_round.lock().await;
        node_id = node_guard.id;
        nodes = node_guard.nodes.clone();
    }

    info!(
        "Node {}: Preparing to send proposals for round {} to nodes: {:?}",
        node_id, round, nodes
    );

    // ✅ **Step 2: Compute Merkle root validation for each transaction**
    // ✅ **Step 2: Compute Merkle root validation for each transaction**
    let data_shards = {
        let node_guard = node.lock().await;
        node_guard.data_shards  // ✅ Fetch from Node struct
    };

    // ✅ Compute hashes for each shard
    let shard_hashes: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| Sha256::digest(shard).to_vec())  // ✅ Flatten nesting issue
        .collect();

    info!("sending proposal Computed shard {:?}", shard_hashes);

    // ✅ Ensure each transaction’s shards are correctly grouped
    let computed_roots: Vec<Vec<u8>> = shard_hashes
        .chunks(data_shards) // ✅ Group shards into transactions
        .map(|shard_group| compute_merkle_root(shard_group))
        .collect();

        info!("sending proposal Computed Merkle roots: {:?}", computed_roots);

    // ✅ Ensure each computed root matches the expected one
    for (i, (computed, provided)) in computed_roots.iter().zip(merkle_roots.iter()).enumerate() {
        if computed != provided {
            return Err(anyhow::anyhow!(
                "Computed Merkle root for transaction {} does not match provided root.\nComputed: {:?}\nProvided: {:?}",
                i, computed, provided
            ));
        }
    }

    // ✅ **Step 5: Compute Merkle branches for each transaction**
// ✅ Compute Merkle branches per transaction
let encoded_proofs: Vec<Vec<Vec<String>>> = shard_hashes
    .chunks(data_shards)  // ✅ Correctly group hashes per transaction
    .map(|tx_shards| {
        tx_shards
            .iter()
            .enumerate()
            .map(|(i, _)| compute_merkle_branch(tx_shards, i))  // ✅ Pass correct slice `&[Vec<u8>]`
            .map(|branch|
                branch.iter()
                    .map(|b| general_purpose::STANDARD.encode(b))  // ✅ Encode proof
                    .collect()
            )
            .collect()
    })
    .collect();

info!("sending proposal encoded_proofs: {:?}", encoded_proofs);



        let data_shards = {
            let node_guard = node.lock().await;
            node_guard.data_shards  // ✅ Get the correct number of shards per transaction
        };

    // ✅ **Create individual proposals per transaction**
    let propose_requests: Vec<ProposeRequest> = (0..merkle_roots.len())
    .map(|i| {
        let start_idx = i * data_shards; // Start of shard group
        let end_idx = start_idx + data_shards; // End of shard group

        ProposeRequest {
            base: BaseRequest {
                proposing_node_id: node_id,
                round_id: round,
                root: merkle_roots[i].clone(),
            },
            proofs: encoded_proofs[i].clone(),  // Correctly assigned proof per transaction
            shards: shards[start_idx..end_idx] // Get the correct `data_shards` chunk per transaction
                .iter()
                .map(|s| general_purpose::STANDARD.encode(s))
                .collect(),
            parents: parent_units.clone(),
        }
    })
    .collect();

    // ✅ **Step 4: Iterate over all nodes `j ∈ N` and send proposals**
    let futures: Vec<_> = nodes.iter().map(|node_url| {
        let client = client.clone();
        let node_url = node_url.clone();
        let proposal = ProposeRequest {
            base: BaseRequest {
                proposing_node_id: node_id,
                round_id: round,
                root: compute_merkle_root(&shard_hashes).clone(),
            },
            proofs: encoded_proofs.iter().map(|tx_proofs| tx_proofs.concat()).collect(),  // ✅ Flattened properly
            shards: shards.iter()
                .map(|s| general_purpose::STANDARD.encode(s))
                .collect(),
            parents: parent_units.clone(),
        };
    
        async move {
            info!(
                "Node {} sending proposal to {} for round {}: {:?}",
                node_id, node_url, proposal.base.round_id, proposal
            );
    
            match client
                .post(format!("http://{}/propose", node_url))
                .json(&proposal)
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    info!("Proposal successfully delivered to Node {} (round {}).", node_url, proposal.base.round_id);
                    Ok(())
                }
                Ok(res) => {
                    let err_msg = format!(
                        "Proposal failed for {}. Status: {}. Response: {}",
                        node_url,
                        res.status(),
                        res.text().await.unwrap_or_else(|_| "No response body".to_string())
                    );
                    error!("{}", err_msg);
                    Err(anyhow::anyhow!(err_msg))
                }
                Err(e) => Err(anyhow::anyhow!("Network error while sending proposal to {}: {:?}", node_url, e)),
            }
        }
    }).collect();
    


    let results: Vec<Result<(), anyhow::Error>> = join_all(futures).await;

    if results.iter().all(|res| res.is_ok()) {
        info!("Successfully sent all proposals for round {}.", round);

        for propose_request in propose_requests {
            if let Err(e) = handle_propose(node.clone(), client.clone(), propose_request).await {
                error!(
                    "Node {}: Failed to handle local proposal. Error: {:?}",
                    node_id, e
                );
            }
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more proposals failed."))
    }
}

