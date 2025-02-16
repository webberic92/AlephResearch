use std::sync::Arc;
use base64::engine::general_purpose;
use base64::Engine;
use tokio::sync::Mutex;
use tracing::{error, info};
use crate::structs::node::{DagUnit, Node, Transaction};
use crate::structs::requests::CommitRequest;
use crate::utils::config_util::write_finalized_dag_to_file;
use chrono::Utc;

/// **🔥 Handles the commit phase in the ch-RBC protocol**  
/// - Manages DAG updates and round progression with `Arc<Mutex<Node>>`.  
///  
/// **ch-RBC Steps:**  
/// - **Step 22:** On `f+1` commit messages, validate DAG updates.  
/// - **Step 23:** Add committed units to DAG.  
/// - **Step 24:** On `2f+1` commits, finalize the DAG.  
/// - **Step 25:** Write finalized DAG to file.  
/// - **Step 26:** Increment the round if conditions are met.
pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let node_id;
    let round_id = commit_request.base.round_id;

    // Step 1: Extract Node ID
    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
    }

    info!(
        "Node {}: Handling commit request from Node {} for round {}",
        node_id, commit_request.base.proposing_node_id, round_id
    );

    // Step 2: Extract Transactions from Commit Payload
    let shard_size = 256;
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
        transactions,
        parent_units: commit_request.parents.clone(),
        merkle_root: general_purpose::STANDARD.encode(&commit_request.base.root),
        finalization_timestamp: Utc::now().timestamp() as u64,
    };

    // Step 3: Lock Node for DAG Updates
    {
        let node_guard = node.lock().await;

        let mut dag = node_guard.dag.lock().await;
        dag.entry(round_id).or_insert_with(Vec::new).push(dag_unit.clone());

        info!(
            "Node {}: Added unit {:?} to DAG at round {}",
            node_guard.id, dag_unit, round_id
        );

        // Step 4: Check if DAG Finalization Condition is Met
        if dag.get(&round_id).map_or(false, |units| units.len() >= node_guard.total_nodes) {
            info!("Node {}: Writing finalized DAG before advancing...", node_id);

            if let Err(e) = write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag, round_id).await {
                error!("Node {}: Failed to write finalized DAG! Error: {:?}", node_id, e);
            } else {
                info!("Node {}: DAG finalized for round {}, now advancing...", node_id, round_id);
            }

            // Step 5: Increment the Round
            let mut current_round_guard = node_guard.current_round.lock().await;
            *current_round_guard += 1;

            info!(
                "Node {}: Local round successfully updated to: {}",
                node_id, *current_round_guard
            );
        }
    }

    info!("Node {}: Exiting commit handler", node_id);
    Ok(())
}
