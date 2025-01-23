use axum::Json;
use base64::{engine::general_purpose, Engine};
use reqwest::Client;
use sha2::Digest;
use std::sync::Arc;
use tracing::{debug, error, info};
use axum::response::IntoResponse;
use crate::{
    handlers::handle_commit::handle_commit,
    structs::{node::Node, requests::PrevoteRequest, responses::Response},
    utils::{
        dag_utils::ensure_dag_synchronization,
        merkle_utils::{reconstruct_unit, validate_merkle_branch},
    },
};

/// Handles a prevote request in the Aleph protocol.
pub async fn handle_prevote(
    node: Arc<Node>,
    client: Arc<Client>,
    payload: PrevoteRequest,
) -> impl IntoResponse {
    info!(
        "Received PrevoteRequest at Node {}: {:?}",
        node.id, payload
    );

    // Step 1: Decode Base64-encoded shards
    let decoded_shards: Vec<Vec<u8>> = match payload
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
    {
        Ok(decoded) => decoded,
        Err(e) => {
            let error_message = format!("Failed to decode shards: {:?}", e);
            error!("{}", error_message);
            return Json(Response {
                status: error_message,
            });
        }
    };

    // Step 2: Decode Base64-encoded proofs
    let decoded_proofs: Vec<Vec<Vec<u8>>> = match payload
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
            return Json(Response {
                status: error_message,
            });
        }
    };

    // Step 3: Validate Merkle Branches for each shard
    for (index, proof) in decoded_proofs.iter().enumerate() {
        if !validate_merkle_branch(&decoded_shards, proof, index, &payload.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}, Proof: {:?}",
                node.id, index, payload.root, proof
            );
            error!("{}", error_message);
            return Json(Response {
                status: error_message,
            });
        }
    }
    info!("Node {}: Merkle branch validation passed.", node.id);

    // Step 4: Ensure DAG synchronization
    if let Err(e) = ensure_dag_synchronization(&node, &client, payload.epoch_id, &payload.senderId, &payload.senderUrl).await {
        let error_message = format!(
            "Node {}: DAG synchronization failed with node {}. Error: {:?}",
            node.id, payload.senderId, e
        );
        error!("{}", error_message);
        return Json(Response {
            status: error_message,
        });
    }
    info!(
        "Node {}: DAG synchronization successful with {}",
        node.id, payload.senderId
    );

    // Step 5: Update quorum votes
    let mut quorum_votes = node.quorum_votes.write().await;
    let count = quorum_votes.entry(payload.root.clone()).or_insert(0);
    *count += 1;

    debug!(
        "Node {}: Updated quorum votes for root {:?}: {}",
        node.id, payload.root, *count
    );

    // Step 6: Check quorum threshold
    if *count < node.get_quorum_threshold() {
        info!(
            "Node {}: Prevote accepted for root {:?}. Current votes: {}",
            node.id, payload.root, *count
        );
        return Json(Response {
            status: format!(
                "Node {}: Prevote accepted for root {:?}",
                node.id, payload.root
            ),
        });
    }

    info!(
        "Node {}: Quorum reached for root {:?} with {} votes",
        node.id, payload.root, *count
    );

    // // Step 7: Reconstruct and commit
    // match reconstruct_unit(&decoded_shards, &decoded_proofs, &payload.root) {
    //     Ok(reconstructed_unit) => {
    //         info!(
    //             "Node {}: Reconstruction successful for root {:?}. Proceeding to commit.",
    //             node.id, payload.root
    //         );
    //         if let Err(e) = handle_commit(
    //             &node,
    //             payload.senderId,
    //             payload.root.clone(),
    //             reconstructed_unit,
    //             payload.epoch_id,
    //         )
    //         .await
    //         {
    //             let error_message = format!(
    //                 "Node {}: Commit phase failed for root {:?}. Error: {:?}",
    //                 node.id, payload.root, e
    //             );
    //             error!("{}", error_message);
    //             return Json(Response {
    //                 status: error_message,
    //             });
    //         }
    //     }
    //     Err(e) => {
    //         let error_message = format!(
    //             "Node {}: Reconstruction failed for root {:?}. Error: {:?}",
    //             node.id, payload.root, e
    //         );
    //         error!("{}", error_message);
    //         return Json(Response {
    //             status: error_message,
    //         });
    //     }
    // }

    info!(
        "Node {}: Successfully handled PREVOTE REQUEST from Node {}",
        node.id, payload.senderId
    );

    Json(Response {
        status: format!(
            "Node {}: Prevote successfully handled for sender Node {}",
            node.id, payload.senderId
        ),
    })
}
