use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use crate::structs::node::Node;
use crate::structs::requests::CommitRequest;
use crate::utils::config_util::write_finalized_dag_to_file;

/// **🔥 Handles the commit phase in the ch-RBC protocol**  
/// - Ensures DAG consistency and safe round progression.
///
/// **ch-RBC Steps:**  
/// - **Step 22:** Validate DAG updates.  
/// - **Step 23:** Add committed units to DAG.  
/// - **Step 24:** Finalize the DAG on `2f+1` commits.  
/// - **Step 25:** Write DAG to file.  
/// - **Step 26:** Increment the round.
/// **🔥 Handles the commit phase in the ch-RBC protocol**  
/// - Ensures DAG consistency and safe round progression.
pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let node_id;
    let round_id = commit_request.base.round_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
    }

    info!("Node {}: Handling commit request for round {}", node_id, round_id);

    {
        let mut node_guard = node.lock().await;
        let mut dag = node_guard.dag.lock().await;

        // Insert all units together
        for unit in &commit_request.units {
            let units = dag.entry(round_id).or_insert_with(Vec::new);
            if !units.iter().any(|u| u.unit_id == unit.unit_id) {
                units.push(unit.clone());
                info!("Node {}: Added unit {} to DAG for round {}", node_id, unit.unit_id, round_id);
            }
        }

        // Check DAG finalization
        let quorum = node_guard.get_quorum_threshold();
        if let Some(units) = dag.get(&round_id) {
            if units.len() >= quorum {
                info!("Node {}: DAG finalized for round {}.", node_id, round_id);
                let _ = write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag, round_id).await;
            }
        }
    }

    // Increment round
    {
        let node_guard = node.lock().await;
        let mut current_round = node_guard.current_round.lock().await;
        if *current_round == round_id {
            *current_round += 1;
            info!("Node {}: Local round advanced to {}", node_id, *current_round);
        }
    }

    info!("Node {}: Exiting commit handler.", node_id);
    Ok(())
}




