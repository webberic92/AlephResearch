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
/// - Ensures DAG consistency and safe round progression.
///
/// **ch-RBC Steps:**  
/// - **Step 22:** Validate DAG updates.  
/// - **Step 23:** Add committed units to DAG.  
/// - **Step 24:** Finalize the DAG on `2f+1` commits.  
/// - **Step 25:** Write DAG to file.  
/// - **Step 26:** Increment the round.
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
        unit_id: format!("U{}-{}", round_id, commit_request.base.proposing_node_id),
        proposer_node: commit_request.base.proposing_node_id,
        round: round_id,
        transactions,
        parent_units: commit_request.parents.clone(),
        merkle_root: general_purpose::STANDARD.encode(&commit_request.base.root),
        finalization_timestamp: Utc::now().timestamp() as u64,
    };

    // Step 3: Lock Node for DAG Updates
    {
        let mut node_guard = node.lock().await;

        let mut dag = node_guard.dag.lock().await;
        let units = dag.entry(round_id).or_insert_with(Vec::new);
        
        // Avoid duplicate entries
        if !units.iter().any(|u| u.unit_id == dag_unit.unit_id) {
            units.push(dag_unit.clone());
            info!("Node {}: Added unit {:?} to DAG at round {}", node_guard.id, dag_unit.unit_id, round_id);
        } else {
            info!("Node {}: Duplicate commit detected. Skipping unit {:?}.", node_guard.id, dag_unit.unit_id);
            return Ok(());
        }

        // Step 4: Check if DAG Finalization Condition is Met
      
        let quorum = node_guard.get_quorum_threshold();

        if units.len() >= quorum {
            info!("Node {}: Quorum reached (≥ {} units). Finalizing DAG.", node_id, quorum);

            // Write finalized DAG to file
            if let Err(e) = write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag, round_id).await {
                error!("Node {}: Failed to write finalized DAG! Error: {:?}", node_id, e);
                return Err(format!("Failed to write finalized DAG: {:?}", e));
            }

            info!("Node {}: DAG finalized for round {}.", node_id, round_id);
        } else {
            info!(
                "Node {}: DAG unit count for round {}: {}/{}. Waiting for more units.",
                node_id, round_id, units.len(), quorum
            );
            return Ok(());
        }
    }

    // Step 5: Increment the Round Safely
    {
        let mut node_guard = node.lock().await;
        let mut current_round = node_guard.current_round.lock().await;

        if *current_round == round_id {
            *current_round += 1;
            info!("Node {}: Local round advanced to {}", node_id, *current_round);
        } else {
            info!("Node {}: Round already advanced.", node_id);
        }
    }

    info!("Node {}: Exiting commit handler.", node_id);
    Ok(())
}

