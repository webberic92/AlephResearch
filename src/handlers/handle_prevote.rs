use axum::Json;
use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::Digest;
use std::sync::Arc;
use tracing::{debug, error, info};
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
            let error_message = format!("Failed to decode shards: {:?}", e);
            error!("{}", error_message);
            return Json(Response { status: error_message });
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
            let error_message = format!("Failed to decode proofs: {:?}", e);
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    };

    // Step 3: ensure dag sync
    if let Err(e) = ensure_dag_synchronization(
        &node,
        &client,
        prevote_request.propose.base.epoch_id,
        &prevote_request.propose.base.sender_id,
        &prevote_request.sender_url,
    )
    .await {
        let error_message = format!(
            "Node {}: DAG synchronization failed with node {}. Error: {:?}",
            node.id, prevote_request.propose.base.sender_id, e
        );
        error!("{}", error_message);
        return Json(Response { status: error_message });
    }
    info!(
        "Node {}: DAG synchronization successful for epoch {}.",
        node.id, prevote_request.propose.base.epoch_id
    );




    // Step 4: Compute shard hashes
    let shard_hashes: Vec<Vec<u8>> = decoded_shards
        .iter()
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    // Step 5: Validate Merkle Branches
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&shard_hashes, proof, index, &prevote_request.propose.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}.",
                node.id, index, prevote_request.propose.base.root
            );
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    }
    // info!("Node {}: Merkle branch validation passed.", node.id);

    // Step 6: Reconstruct the unit
    let parents = shard_hashes
        .iter()
        .flat_map(|hash| hash.clone()) // Flatten to a single Vec<u8>
        .collect();

    let reconstructed_unit = match reconstruct_unit(
        &decoded_shards,
        prevote_request.propose.base.epoch_id,
        parents,
    ) {
        Ok(reconstructed_unit) => reconstructed_unit,
        Err(e) => {
            let error_message = format!(
                "Node {}: Reconstruction failed for root {:?}. Error: {:?}",
                node.id, prevote_request.propose.base.root, e
            );
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }

    };

            info!(
            "Node {}: Successfully reconstructed unit for root {:?} UNIT : {:?}.",
            node.id, prevote_request.propose.base.root, reconstructed_unit
        );
    // Step 7: Validate parents
    if let Err(e) = validate_unit_parents(&node, &reconstructed_unit.data).await {
        let error_message = format!(
            "Node {}: Parent validation failed for root {:?}. Error: {}",
            node.id, prevote_request.propose.base.root, e
        );
        error!("{}", error_message);
        return Json(Response { status: error_message });
    }

    // Step 8: Check quorum threshold and commit
    let mut quorum_votes = node.quorum_votes.write().await;
    let count = quorum_votes
        .entry(prevote_request.propose.base.root.clone())
        .or_insert(0);
    *count += 1;

    debug!(
        "Node {}: Updated quorum votes for root {:?}: {}",
        node.id, prevote_request.propose.base.root, *count
    );

    if *count >= node.get_quorum_threshold() {
        // Proceed to commit phase
        let commit_request = CommitRequest {
            base: prevote_request.propose.base.clone(),
            unit: reconstructed_unit.data.clone(), // Use the `data` field of the reconstructed unit
            proofs: decoded_proofs
                .iter()
                .map(|proof| proof.iter().map(|p| general_purpose::STANDARD.encode(p)).collect())
                .collect(),
        };

        if let Err(e) = handle_commit(&node, commit_request).await {
            let error_message = format!(
                "Node {}: Commit phase failed for root {:?}. Error: {:?}",
                node.id, prevote_request.propose.base.root, e
            );
            error!("{}", error_message);
            return Json(Response { status: error_message });
        }
    }

    // info!(
    //     "Node {}: Successfully handled PREVOTE REQUEST from Node {}.",
    //     node.id, prevote_request.propose.base.sender_id
    // );

    Json(Response {
        status: format!(
            "Node {}: Prevote successfully handled for sender Node {}.",
            node.id, prevote_request.propose.base.sender_id
        ),
    })
}
