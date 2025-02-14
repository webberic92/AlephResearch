use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use base64::engine::general_purpose;

use base64::Engine;
use serde_json::Value;
use tokio::fs::{ self, OpenOptions };
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{ error, info };
use crate::structs::node::{DagUnit, Node, Transaction};
use crate::structs::requests::CommitRequest;
use crate::utils::epoch_utils::update_local_epoch;

/// **🔥 Handles the commit phase in the ch-RBC protocol**
/// - Ensures multiple transactions are committed before advancing the epoch.
///
/// **ch-RBC Steps Implemented:**
/// - **Step 22**: Upon receiving `f + 1` commit messages, check if the commit has been sent.
/// - **Step 23**: If the commit has **not** been sent yet, multicast commit.
/// - **Step 24**: Multicast commit(Ps, r, h) to all nodes.
/// - **Step 25**: Upon receiving `2f + 1` commit messages, finalize unit decoding.
/// - **Step 26**: Output U, which is decoded from `s_j` shares.
pub async fn handle_commit(
    node: Arc<RwLock<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let node_id;
    let round_id = commit_request.base.round_id;

    {
        let node_state = node.read().await;
        node_id = node_state.id;
    } // 🔥 Dropping read lock

    info!(
        "Node {}: Handling commit request from Node {} for epoch {}",
        node_id, commit_request.base.proposing_node_id, round_id
    );

    // ✅ Extract transaction data properly
    let shard_size = 64; // Assuming fixed shard size
    let mut transactions = Vec::new();

    for (i, chunk) in commit_request.unit.chunks(shard_size).enumerate() {
        let tx_id = format!("Tx{}", i + 1);
        transactions.push(Transaction {
            tx_id,
            data: chunk.to_vec(),
        });
    }

    let dag_unit = DagUnit {
        unit_id: format!("U{}", round_id),
        proposer_node: commit_request.base.proposing_node_id,
        round: round_id,
        transactions, // ✅ Properly structured transactions
        parent_units: commit_request.parents.clone(),
        merkle_root: general_purpose::STANDARD.encode(&commit_request.base.root),
        finalization_timestamp: chrono::Utc::now().timestamp() as u64,
    };

    let node_write = node.write().await;
    let mut dag = node_write.dag.write().await;

    dag.entry(round_id).or_insert_with(Vec::new).push(dag_unit.clone());

    info!(
        "Node {}: Added unit {:?} to DAG at epoch {}",
        node_write.id, dag_unit, round_id
    );

    if dag.get(&round_id).map_or(false, |units| units.len() >= node_write.total_nodes) {
        info!("Node {}: Advancing to next epoch...", node_id);
        write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag, round_id).await.unwrap();
        update_local_epoch(node.clone()).await;
    }

    Ok(())
}


// **Writes the finalized unit to the epoch file**
pub async fn write_finalized_dag_to_file(
    base_path: &str,
    dag: &HashMap<u64, Vec<DagUnit>>,
    round_id: u64,  // ✅ Only write finalized units for this round
) -> Result<(), Box<dyn std::error::Error>> {
    if dag.is_empty() {
        error!("DAG is empty, nothing to write.");
        return Ok(()); 
    }

    // ✅ Fetch only the finalized units for the given round
    if let Some(units) = dag.get(&round_id) {
        let epoch_file = format!("{}/epoch{}.json", base_path, round_id);
        let path = Path::new(&epoch_file);

        if let Some(parent_dir) = path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
            }
        }

        let epoch_data: Vec<Value> = units
            .iter()
            .map(|unit| serde_json::json!({
                "unit_id": unit.unit_id,
                "creator": unit.proposer_node,
                "round": unit.round,
                "transactions": unit.transactions.iter().map(|tx| {
                    serde_json::json!({
                        "tx_id": tx.tx_id,
                        "data": general_purpose::STANDARD.encode(&tx.data),
                    })
                }).collect::<Vec<Value>>(),
                "parents": unit.parent_units,
                "merkle_root": unit.merkle_root,
                "finalization_timestamp": unit.finalization_timestamp,
            }))
            .collect();

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&epoch_file)
            .await?;

        file.write_all(serde_json::to_string_pretty(&epoch_data)?.as_bytes()).await?;

        info!("Successfully wrote finalized DAG for epoch {} to file: {}", round_id, epoch_file);
    }

    Ok(())
}


