use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use reqwest::Client;
use serde_json::Value;
use sha2::Digest;
use tokio::fs::{ self, OpenOptions };
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{ error, info };
use crate::structs::node::{DagUnit, Node};
use crate::structs::requests::CommitRequest;
// use crate::utils::dag_utils::are_parents_available;
use crate::utils::epoch_utils::{ broadcast_epoch_update, update_local_epoch };
use crate::utils::merkle_utils::validate_merkle_branch;

/// **🔥 Handles the commit phase in the Aleph protocol**
/// - Ensures multiple transactions are committed before advancing the epoch.
pub async fn handle_commit(
    node: Arc<RwLock<Node>>,
    // client: Arc<Client>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    // Step 1: Read Node ID (Drop lock after reading)
    let node_id;
    let epoch_id = commit_request.base.epoch_id; // ✅ Store epoch ID before acquiring write lock

    {
        let node_state = match timeout(Duration::from_secs(5), node.read()).await {
            Ok(state) => state,
            Err(_) => {
                error!("Timeout while acquiring read lock commit step 1!");
                return Err("Timeout while acquiring read lock commit step 1".to_string());
            }
        };
        node_id = node_state.id;
    } // 🔥 Dropping read lock

    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node_id, commit_request.base.proposing_node_id, epoch_id
    );

    // Step 2: Validate Merkle branches (NO LOCK HELD)
    let shard_size = 64;
    let shard_hashes: Vec<Vec<u8>> = commit_request
        .unit
        .chunks(shard_size)
        .map(|shard| sha2::Sha256::digest(shard).to_vec())
        .collect();

    for (index, proof) in commit_request.proofs.iter().enumerate() {
        let decoded_proof: Vec<Vec<u8>> = proof
            .iter()
            .map(|p| base64::engine::general_purpose::STANDARD.decode(p.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|e| {
                let error_message = format!(
                    "Node {}: Failed to decode proof {}: {:?}",
                    node_id, index, e
                );
                error!("{}", error_message);
                error_message
            })?;

        if !validate_merkle_branch(&shard_hashes, &decoded_proof, index, &commit_request.base.root) {
            let error_message = format!(
                "Node {}: Merkle root mismatch for shard {}. Expected root: {:?}",
                node_id, index, commit_request.base.root
            );
            error!("{}", error_message);
            return Err(error_message);
        }
    }

    // Step 3: Check parent availability (NO LOCK HELD)
    // if !are_parents_available(node.clone(), &commit_request.unit).await {
    //     let error_message = format!(
    //         "Node {}: Parent availability check failed for root {:?}",
    //         node_id, commit_request.base.root
    //     );
    //     error!("{}", error_message);
    //     return Err(error_message);
    // }

    let should_advance_epoch;

    {
        let node_read = node.read().await;
    
        // Step 4: Compute next unit ID and parents **before acquiring write lock**
        let next_unit_id = node_read.get_next_dag_unit_id(epoch_id).await;
        let parent_units = node_read.get_next_parents(epoch_id).await;
    
        drop(node_read); // 🔴 Release read lock ASAP
    
        let dag_unit = DagUnit {
            unit_id: next_unit_id,
            proposer_node: commit_request.base.proposing_node_id,
            data: commit_request.unit.clone(),
            parent_units,
            finalization_timestamp: chrono::Utc::now().timestamp() as u64,
        };
    
        let node_write = node.write().await;
        let mut dag = node_write.dag.write().await;
    
        // ✅ Ensure there's an entry for the current epoch
        dag.entry(epoch_id).or_insert_with(Vec::new).push(dag_unit.clone());
    
        info!(
            "Node {}: Added unit {:?} to DAG at epoch {}",
            node_write.id, dag_unit, epoch_id
        );
    
        info!("Node {}: Current DAG VALUE: {:?}", node_write.id, dag.get(&epoch_id));
    
        // ✅ Ensure `get()` safely handles missing epochs
        should_advance_epoch = dag.get(&epoch_id).map_or(false, |units| units.len() >= node_write.total_nodes);
    } // 🔴 Drop write lock immediately
    




    // ✅ Step 6: Update epoch and broadcast (NO LOCKS HELD)
    if should_advance_epoch {

        {
            let dag_clone;
            {
                let node_read = node.read().await;
                let dag_read = node_read.dag.read().await;
                dag_clone = dag_read.clone(); // ✅ Clone the DAG before releasing the lock
            } // 🔴 Drop the read lock ASAP
        
            if let Err(e) = write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag_clone).await {
                error!("Failed to write finalized DAG: {:?}", e);
            }

        }
        
        info!("Node {}: Advancing to next epoch...", node_id);
        let new_epoch_id = update_local_epoch(node.clone()).await; // ✅ No locks held here

        // if let Err(e) = broadcast_epoch_update(node.clone(), client.clone(), new_epoch_id).await {
        //     error!("Node {}: Failed to broadcast epoch update: {}", node_id, e);
        // } else {
        //     info!("Node {}: Successfully broadcasted epoch update.", node_id);
        // }
    }

    info!("Node {}: Successfully handled commit request.", node_id);
    Ok(())
}


// **Writes the finalized unit to the epoch file**
// **Writes the entire DAG to epoch-specific files**
pub async fn write_finalized_dag_to_file(
    base_path: &str,
    dag: &HashMap<u64, Vec<DagUnit>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // ✅ Log the entire DAG before writing
    if dag.is_empty() {
        error!("DAG is empty, nothing to write.");
        return Ok(()); // ✅ Early return if DAG is empty
    }

    info!("Starting DAG write process. Total epochs: {}", dag.len());

    for (&epoch_id, units) in dag.iter() {
        let epoch_file = format!("{}/epoch{}.json", base_path, epoch_id);
        let path = Path::new(&epoch_file);

        info!(
            "Writing DAG for epoch {}. Total units in dag: {}",
            epoch_id,
            units.len()
        );

        // ✅ Ensure directory exists
        if let Some(parent_dir) = path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
                info!("Created directory for DAG storage: {:?}", parent_dir);
            }
        }

        // Convert all units in the epoch to JSON format
        let epoch_data: Vec<Value> = units
            .iter()
            .map(|unit| serde_json::json!({
                "unit_id": unit.unit_id,
                "proposer_node": unit.proposer_node,
                "data": unit.data,
                "parent_units": unit.parent_units,
                "finalization_timestamp": unit.finalization_timestamp
            }))
            .collect();

        // Open file and write epoch DAG
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&epoch_file)
            .await?;

        file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes())
            .await?;

        info!(
            "Successfully wrote finalized DAG for epoch {} to file: {}",
            epoch_id, epoch_file
        );
    }

    Ok(())
}

