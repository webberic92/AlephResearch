use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};
use crate::structs::node::Node;
use crate::structs::requests::CommitRequest;
use crate::utils::config_util::write_finalized_dag_to_file;

/// **🔥 Handles the commit phase in the ch-RBC protocol**  
/// - Ensures DAG consistency and safe round progression.
///
/// **ch-RBC Steps:**  
/// - **Step 22:** Upon receiving `f+1` commits, validate and track the commit.  
/// - **Step 23:** Check if commit has already been sent; if not, multicast it.  
/// - **Step 24:** Finalize the DAG when `2f+1` commits are received.  
/// - **Step 25:** Write finalized DAG to file.  
/// - **Step 26:** Increment the round.
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

    info!("============== Node {}: Handling commit request for round {}==============", node_id, round_id);

    // Step 22: Upon receiving f+1 commits, insert units into the DAG.
    {
        let node_guard = node.lock().await;
        let mut dag = node_guard.dag.lock().await;

        for unit in &commit_request.units {
            let units = dag.entry(round_id).or_insert_with(Vec::new);
            if !units.iter().any(|u| u.unit_id == unit.unit_id) {
                units.push(unit.clone());
                info!("Node {}: Added unit {} to DAG for round {}", node_id, unit.unit_id, round_id);
            }
        }

        // Step 23: Check if commit has already been sent
        let mut commit_tracker = node_guard.commit_tracker.lock().await;
        let commit_key = format!("{}-{}", commit_request.base.proposing_node_id, round_id);
        if commit_tracker.contains(&commit_key) {
            info!("Node {}: Commit message for round {} already sent.", node_id, round_id);
            return Ok(());
        }

        // Mark this commit as sent
        commit_tracker.insert(commit_key.clone());

        // Step 24: Check if we have reached `2f+1` commits
        let quorum = node_guard.get_quorum_threshold();
        if let Some(units) = dag.get(&round_id) {
            if units.len() >= quorum {
                info!("Node {}: DAG finalized for round {}.", node_id, round_id);

                // Step 25: Write finalized DAG to file
                if let Err(e) = write_finalized_dag_to_file("/home/aleph-node/logs/finalized_dag", &dag, round_id).await {
                    error!("Node {}: Failed to write finalized DAG: {:?}", node_id, e);
                    return Err(format!("Failed to write finalized DAG: {:?}", e));
                }

                // Step 23 (continued): Multicast commit message to all nodes
                let node_ips = &node_guard.nodes;
                let commit_message = serde_json::to_string(&commit_request).map_err(|e| format!("Serialization failed: {:?}", e))?;
                for target_node in node_ips {
                    let target_url = format!("http://{}/commit", target_node);
                    let client = reqwest::Client::new();
                    match client.post(&target_url)
                        .body(commit_message.clone())
                        .send()
                        .await {
                        Ok(response) if response.status().is_success() => {
                            info!("Node {}: Successfully sent commit to {}", node_id, target_url);
                        }
                        Ok(response) => {
                            error!("Node {}: Failed to send commit to {}. Status: {}", node_id, target_url, response.status());
                        }
                        Err(e) => {
                            error!("Node {}: Network error while sending commit to {}: {:?}", node_id, target_url, e);
                        }
                    }
                }
            } else {
                info!("Node {}: Not enough commits yet. Waiting for 2f+1 commits.", node_id);
            }
        }
    }

    // Step 26: Increment the round
    {
        let node_guard = node.lock().await;
        let mut current_round = node_guard.current_round.lock().await;
        if *current_round == round_id {
            *current_round += 1;
            info!("Node {}: Local round advanced to {}", node_id, *current_round);
        }
    }

    info!("==============Node {}: Exiting commit handler.==============", node_id);
    Ok(())
}
