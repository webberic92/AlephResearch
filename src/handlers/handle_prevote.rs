use axum::Json;
use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::Digest;
use std::sync::Arc;
use tracing::{ error, info};
use axum::response::IntoResponse;
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{
        node::Node,
        requests::{CommitRequest, PrevoteRequest},
        responses::Response,
    },
    utils::{
        dag_utils::{validate_unit_parents, ensure_dag_synchronization},
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

/// Handles a PREVOTE request.
///
/// # Parameters
/// - `node`: The current node.
/// - `client`: HTTP client for making network requests.
/// - `prevote_request`: The received PREVOTE request.
///
/// # Returns
/// - JSON response indicating success or failure.
pub async fn handle_prevote(
    node: Arc<Node>,
    client: Arc<Client>,
    prevote_request: PrevoteRequest,
) -> impl IntoResponse {
    info!(
        "Node {} {}: Received PREVOTE request from Node {} at {} for epoch {}.",
        node.id,
        node.ip_address,
        prevote_request.propose.base.sender_id,
        prevote_request.sender_url,
        prevote_request.propose.base.epoch_id
    );

    // Step 1: Decode Base64-encoded shards
    let decoded_shards = match prevote_request
        .propose
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
    {
        Ok(shards) => shards,
        Err(e) => {
            error!("Node {}: Failed to decode shards: {:?}", node.id, e);
            return Json(Response { status: format!("Shard decoding failed: {:?}", e) });
        }
    };

    // Step 2: Decode Base64-encoded proofs
    let decoded_proofs = match prevote_request
        .propose
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
        Ok(proofs) => proofs,
        Err(e) => {
            error!("Node {}: Failed to decode proofs: {:?}", node.id, e);
            return Json(Response { status: format!("Proof decoding failed: {:?}", e) });
        }
    };

    // Step 3: Ensure DAG synchronization
    if let Err(e) = ensure_dag_synchronization(
        &node,
        &client,
        prevote_request.propose.base.epoch_id,
        &prevote_request.propose.base.sender_id,
        &prevote_request.sender_url,
    )
    .await
    {
        error!("Node {}: DAG synchronization failed: {:?}", node.id, e);
        return Json(Response { status: format!("DAG sync failed: {:?}", e) });
    }
    info!("Node {}: DAG synchronization successful.", node.id);

    // Step 4: Compute shard hashes
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // Step 5: Validate Merkle branches
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            error!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}.",
                node.id, index, prevote_request.propose.base.root
            );
            return Json(Response {
                status: "Merkle root mismatch.".to_string(),
            });
        }
    }

    // Step 6: Reconstruct the unit
    let parents = shard_hashes.iter().flat_map(|hash| hash.clone()).collect();
    let reconstructed_unit = match reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.epoch_id,
        parents,
    ) {
        Ok(unit) => unit,
        Err(e) => {
            error!(
                "Node {}: Reconstruction failed. Error: {:?}",
                node.id, e
            );
            return Json(Response {
                status: format!("Reconstruction failed: {:?}", e),
            });
        }
    };

    // Step 7: Validate parents
    if let Err(e) = validate_unit_parents(&node, &reconstructed_unit.data).await {
        error!(
            "Node {}: Parent validation failed. Error: {:?}",
            node.id, e
        );
        return Json(Response {
            status: format!("Parent validation failed: {:?}", e),
        });
    }

    // Step 8: Quorum check and commit
    let mut quorum_votes = node.quorum_votes.write().await;
    let count = quorum_votes
        .entry(prevote_request.propose.base.root.clone())
        .or_insert(0);
    *count += 1;

    info!(
        "Node {}: Quorum votes updated for root {:?}: {}.",
        node.id, prevote_request.propose.base.root, *count
    );

    if *count >= node.get_quorum_threshold() {
        info!(
            "Node {}: Quorum threshold met. Preparing commit for root {:?}.",
            node.id, prevote_request.propose.base.root
        );

        let commit_request = CommitRequest {
            base: prevote_request.propose.base.clone(),
            unit: reconstructed_unit.data.clone(),
            proofs: decoded_proofs
                .iter()
                .map(|proof| {
                    proof
                        .iter()
                        .map(|p| general_purpose::STANDARD.encode(p))
                        .collect()
                })
                .collect(),
        };

        if let Err(e) = handle_commit(&node, client, commit_request).await {
            error!(
                "Node {}: Commit phase failed. Error: {:?}",
                node.id, e
            );
            return Json(Response {
                status: format!("Commit failed: {:?}", e),
            });
        }
    }

    Json(Response {
        status: format!(
            "Node {}: Prevote successfully handled.",
            node.id
        ),
    })
}

