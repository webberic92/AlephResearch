use axum::Json;
use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::Digest;
use std::sync::Arc;
use tracing::{debug, error, info};
use axum::response::IntoResponse;
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{node::Node, requests::{CommitRequest, PrevoteRequest}, responses::Response},
    utils::{
        dag_utils::ensure_dag_synchronization,
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

pub async fn handle_prevote(
    node: Arc<Node>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,
) -> impl IntoResponse {
    info!(
        " Node {} {} : Received PrevoteRequest from node {} : {} with epoch {:?}",
        node.id, node.ip_address, prevote_request.propose.base.sender_id, prevote_request.sender_url, prevote_request.propose.base.epoch_id
    );

    // Step 1: Decode Base64-encoded shards
    let decoded_shards: Vec<Vec<u8>> = match prevote_request.propose
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
    {
        Ok(decoded) => decoded,
        Err(e) => {
            let error_message = format!("Failed to decode shards: {:?}", e);
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    };

    // Step 2: Decode Base64-encoded proofs
    let decoded_proofs: Vec<Vec<Vec<u8>>> = match prevote_request.propose
        .proofs
        .iter()
        .map(|proof| {
            proof
                .iter()
                .map(|p| general_purpose::STANDARD.decode(p.as_bytes()))
                .collect::<Result<Vec<_>, _>>() // Decode individual branch
        })
        .collect::<Result<Vec<_>, _>>() // Collect all decoded branches
    {
        Ok(decoded) => decoded,
        Err(e) => {
            let error_message = format!("Failed to decode proofs: {:?}", e);
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    };


    info!(
        "Prevote Phase - Node {}: Shards: {:?}, Proofs: {:?}, Root: {:?}",
        node.id, decoded_shards, decoded_proofs, prevote_request.propose.base.root
    );


    
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
    .iter()
    .map(|shard| sha2::Sha256::digest(shard).to_vec())
    .collect();

    info!(
        "Node {}: handle prevotes (validate_merkle_branch)  DOES NOT work  shard hashes {:?} decoded proofs, {:?}, root {:?}",
        node.id, shard_hashes, decoded_proofs, prevote_request.propose.base.root
    );
    // Step 3: Validate Merkle Branches for each shard
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}, Proof: {:?}",
                node.id, index, prevote_request.propose.base.root, proof
            );
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    }
    info!("Node {}: Merkle branch validation passed.", node.id);

    // Step 4: Ensure DAG synchronization
    if let Err(e) = ensure_dag_synchronization(&node, &client, prevote_request.propose.base.epoch_id, &prevote_request.propose.base.sender_id, &prevote_request.sender_url).await {
        let error_message = format!(
            "Node {}: DAG synchronization failed with node {}. Error: {:?}",
            node.id, prevote_request.propose.base.sender_id, e
        );
        error!("{}", error_message);
        return Json(Response { status: error_message });
    }
    info!(
        "Node {}: DAG synchronization successful with {}",
        node.id, prevote_request.propose.base.sender_id
    );

    // Step 5: Update quorum votes
    let mut quorum_votes = node.quorum_votes.write().await;
    let count = quorum_votes.entry(prevote_request.propose.base.root.clone()).or_insert(0);
    *count += 1;

    debug!(
        "Node {}: Updated quorum votes for root {:?}: {}",
        node.id, prevote_request.propose.base.root, *count
    );

    // Step 6: Check quorum threshold
    if *count < node.get_quorum_threshold() {
        info!(
            "Node {}: Prevote accepted for root {:?}. Current votes: {}",
            node.id, prevote_request.propose.base.root, *count
        );
        return Json(Response {
            status: format!(
                "Node {}: Prevote accepted for root {:?}",
                node.id, prevote_request.propose.base.root
            ),
        });
    }

    info!(
        "Node {}: Quorum reached for root {:?} with {} votes",
        node.id, prevote_request.propose.base.root, *count
    );

    // Step 7: Reconstruct and commit
    // let shard_hashes: Vec<Vec<u8>> = decoded_shards
    //     .iter()
    //     .map(|shard| sha2::Sha256::digest(shard).to_vec())
    //     .collect();

    match reconstruct_unit(&decoded_shards, &decoded_proofs, &prevote_request.propose.base.root) {
        Ok(reconstructed_unit) => {
            info!(
                "Node {}: Reconstruction successful for root {:?}. Proceeding to commit.",
                node.id, prevote_request.propose.base.root
            );
    
            let proofs: Vec<Vec<String>> = decoded_proofs
                .iter()
                .map(|proof| {
                    proof
                        .iter()
                        .map(|p| general_purpose::STANDARD.encode(p))
                        .collect()
                })
                .collect();
            
            let commit_request = CommitRequest {
                base: prevote_request.propose.base.clone(),
                unit: reconstructed_unit,
                proofs,
            };
    
            if let Err(e) = handle_commit(
                &node,
                client.clone(),
                commit_request.clone()
            ).await {
                let error_message = format!(
                    "Node {}: Commit phase failed for root {:?}. Error: {:?}",
                    node.id, prevote_request.propose.base.root, e
                );
                error!("{}", error_message);
                return Json(Response { status: error_message });
            }
    
            // Epoch transition logic
            let current_epoch = prevote_request.propose.base.epoch_id;
            let new_epoch = current_epoch + 1;
    
            info!(
                "Node {}: Finalized Epoch {}. Transitioning to Epoch {}.",
                node.id, current_epoch, new_epoch
            );
    
            // Broadcast new epoch to other nodes
            // for node_url in node..nodes.clone() {
                let sync_url = format!("{}/sync_epoch", node.ip_address);
                let prevote_request = serde_json::json!({
                    "epoch_id": new_epoch,
                    "sender": node.id,
                });
    
                match client.post(&sync_url).json(&prevote_request).send().await {
                    Ok(response) if response.status().is_success() => {
                        info!("Successfully synced epoch {} with node at {}", new_epoch, node.ip_address);
                    }
                    Ok(response) => {
                        error!(
                            "Failed to sync epoch {} with node at {}: HTTP {}",
                            new_epoch, node.ip_address, response.status()
                        );
                    }
                    Err(e) => {
                        error!(
                            "Error syncing epoch {} with node at {}: {:?}",
                            new_epoch, node.ip_address, e
                        );
                    }
                }
            // }
        }
        Err(e) => {
            let error_message = format!(
                "Node {}: Reconstruction failed for root {:?}. Error: {:?}",
                node.id, prevote_request.propose.base.root, e
            );
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    }

    info!(
        "Node {}: Successfully handled PREVOTE REQUEST from Node {}",
        node.id, prevote_request.propose.base.sender_id
    );

    Json(Response {
        status: format!(
            "Node {}: Prevote successfully handled for sender Node {}",
            node.id, prevote_request.propose.base.sender_id
        ),
    })
}


