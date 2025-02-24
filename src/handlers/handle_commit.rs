use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};
use crate::{
    structs::{node::Node, requests::CommitRequest},
    utils::{config_util::write_finalized_dag_to_file, events::Event},
};

pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let (node_id, round_id) = {
        let node_guard = node.lock().await;
        (node_guard.id, commit_request.round_id)
    };

    info!(
        "============== Node {}: Handling commit request from {} for round {} ==============",
        node_id, commit_request.proposing_node_id, round_id
    );

    // ✅ Exit early if the round is already finalized
    {
        let node_guard = node.lock().await;
        let dag_guard = node_guard.dag.lock().await;

        if dag_guard.contains_key(&round_id) {
            info!(
                "Node {}: Commit for round {} already finalized. Ignoring duplicate commit request.",
                node_id, round_id
            );
            return Ok(());  
        }
    } 

    let commit_count;
    let quorum_threshold;

    {
        let node_guard = node.lock().await;
        let mut commit_tracker = node_guard.commit_tracker.lock().await;

        // ✅ Ensure each proposer commits only once per round
        let round_commits = commit_tracker.entry(round_id).or_insert_with(Vec::new);
        if round_commits.iter().any(|c| c.proposing_node_id == commit_request.proposing_node_id) {
            info!(
                "Node {}: Duplicate commit from {} for round {}. Ignoring.",
                node_id, commit_request.proposing_node_id, round_id
            );
            return Ok(());
        }

        round_commits.push(commit_request.clone());
        commit_count = round_commits.len();
        quorum_threshold = node_guard.get_quorum_threshold();
    } 

    info!(
        "Node {}: Commit count for round {} is {}/{}.",
        node_id, round_id, commit_count, quorum_threshold
    );

    if commit_count >= quorum_threshold {
        info!(
            "Node {}: Finalizing round {} with {}/{} commits.",
            node_id, round_id, commit_count, quorum_threshold
        );

        let all_commits;
        {
            let node_guard = node.lock().await;
            let mut commit_tracker = node_guard.commit_tracker.lock().await;
            all_commits = commit_tracker.remove(&round_id).unwrap_or_default();
        }

        let mut all_units = Vec::new();
        for commit in &all_commits {
            all_units.extend(commit.units.clone());
        }

        {
            let node_guard = node.lock().await;
            let mut dag = node_guard.dag.lock().await;
            let dag_units = dag.entry(round_id).or_insert_with(Vec::new);

            for unit in all_units {
                let unit_merkle_root = &unit.merkle_root;

                if !dag_units.iter().any(|u| u.merkle_root == *unit_merkle_root) {
                    dag_units.push(unit.clone());
                }
            }
        }

        let finalized_dag = {
            let node_guard = node.lock().await;
            let dag_guard = node_guard.dag.lock().await;
            dag_guard.clone()  
        }; 

        if let Err(e) = write_finalized_dag_to_file(
            "/home/aleph-node/logs/finalized_dag",
            &finalized_dag,
            round_id,
        )
        .await
        {
            error!(
                "Node {}: Failed to write finalized DAG: {:?}",
                node_id, e
            );
            return Err(format!("Failed to write finalized DAG: {:?}", e));
        }

        // ✅ **Step 26:** Increment the round if applicable
        {
            let node_guard = node.lock().await;
            let mut current_round = node_guard.current_round.lock().await;
            if *current_round == round_id {
                *current_round += 1;
                info!(
                    "Node {}: Local round advanced to {}",
                    node_id, *current_round
                );
            }
        }

        // ✅ **Emit RoundFinalized Event** when the round is finalized
        {
            let node_guard = node.lock().await;
            if let Err(e) = node_guard.event_sender.send(Event::RoundFinalized(round_id)).await {
                error!("Node {}: Failed to send RoundFinalized event: {:?}", node_id, e);
            } else {
                info!("Node {}: Sent RoundFinalized event for round {}", node_id, round_id);
            }
        }
    } else {
        info!(
            "Node {}: Not enough commits yet. Waiting for {}/{} commits.",
            node_id, commit_count, quorum_threshold
        );
    }

    info!(
        "============== Node {}: Exiting commit handler.==============",
        node_id
    );
    Ok(())
}
