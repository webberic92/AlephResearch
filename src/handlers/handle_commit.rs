use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, error};
use reqwest::Client;
use crate::{
    structs::{node::Node, requests::CommitRequest},
    utils::config_util::write_finalized_dag_to_file,
};

pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let node_id;
    let round_id = commit_request.round_id;

    {
        let node_guard = node.lock().await;
        node_id = node_guard.id;
    }

    info!(
        "============== Node {}: Handling commit request from {} for round {} ==============",
        node_id,commit_request.proposing_node_id, round_id
    );
    // info!(
    //     "Node {}: Received commit from {} : {:?}",
    //     node_id, commit_request.proposing_node_id, commit_request.units
    // );
    {
        let node_guard = node.lock().await;
        let dag_guard = node_guard.dag.lock().await;
    
        if dag_guard.contains_key(&round_id) {
            info!(
                "Node {}: Commit for round {} already finalized. Ignoring duplicate commit request.",
                node_id, round_id
            );
            return Ok(());  // ✅ Exit early if this round is already committed
        }
    } // 🔓 Release lock before proceeding
    



    let commit_count;

    {
        let node_guard = node.lock().await;
        let mut commit_tracker = node_guard.commit_tracker.lock().await;

        // ✅ **Ensure `commit_tracker` is a `HashMap<u64, Vec<CommitRequest>>`**
        let round_commits = commit_tracker.entry(round_id).or_insert_with(Vec::new);

        // ✅ **Ensure each proposer commits only once per round**
        if round_commits.iter().any(|c| c.proposing_node_id == commit_request.proposing_node_id) {
            info!(
                "Node {}: Duplicate commit from {} for round {}. Ignoring.",
                node_id, commit_request.proposing_node_id, round_id
            );
            return Ok(());
        }

        // ✅ Store the commit request in commit_tracker
        round_commits.push(commit_request.clone());
        commit_count = round_commits.len();
    } // 🔓 Release commit_tracker lock

    let quorum = {
        let node_guard = node.lock().await;
        node_guard.get_quorum_threshold()
    };

    info!(
        "Node {}: Commit count for round {} is {}/{}.",
        node_id, round_id, commit_count, quorum
    );

    // ✅ **Step 24: If quorum is met, finalize DAG**
    if commit_count >= quorum {
        info!(
            "Node {}: Finalizing round {} with {}/{} commits.",
            node_id, round_id, commit_count, quorum
        );

        // **Step 25:** Retrieve ALL stored commits for this round
        let all_commits;
        {
            let node_guard = node.lock().await;
            let mut commit_tracker = node_guard.commit_tracker.lock().await;
            all_commits = commit_tracker.remove(&round_id).unwrap_or_default();
        } // 🔓 Release commit_tracker lock


        // info!("All commits: {:?}", all_commits);

        // ✅ **Insert ALL committed units into the DAG**
        let mut all_units = Vec::new();
        for commit in &all_commits {
            all_units.extend(commit.units.clone());
        }

        {
            let node_guard = node.lock().await;
            let mut dag = node_guard.dag.lock().await;

            let dag_units = dag.entry(round_id).or_insert_with(Vec::new);

            // info!("DAG units: {:?}", dag_units);
            // info!("All units: {:?}", all_units);

            for unit in all_units {
                let unit_merkle_root = &unit.merkle_root;

                // ✅ **Ensure unit isn't already in DAG**
                if !dag_units.iter().any(|u| u.merkle_root == *unit_merkle_root) {
                    dag_units.push(unit.clone());
                    // info!(
                    //     "Node {}: Added committed unit with Merkle root {:?} to DAG for round {}",
                    //     node_id, unit_merkle_root, round_id
                    // );
                }
            }
        }

        // ✅ **Step 25: Write finalized DAG to file**
        let finalized_dag = {
            let node_guard = node.lock().await;
            let dag_guard = node_guard.dag.lock().await;
            dag_guard.clone()  // ✅ Clone the DAG so we can use it outside the lock
        }; // 🔓 Lock is released here
        // info!("Finalized DAG: {:?}", finalized_dag);
        if let Err(e) = write_finalized_dag_to_file(
            "/home/aleph-node/logs/finalized_dag",
            &finalized_dag,  // ✅ Use the cloned DAG
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
    } else {
        info!(
            "Node {}: Not enough commits yet. Waiting for {}/{} commits.",
            node_id, commit_count, quorum
        );
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

    info!(
        "============== Node {}: Exiting commit handler.==============",
        node_id
    );
    Ok(())
}
