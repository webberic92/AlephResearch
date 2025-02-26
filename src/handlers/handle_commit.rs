use std::sync::Arc;
use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{info, error};
use crate::{
    processors::{priority_queue::RBCMessage, rbc_processor::RBCProcessor}, structs::{node::Node, requests::CommitRequest}, utils::{config_util::write_finalized_dag_to_file, events::Event}
};

pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    client: Arc<Client>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let (node_id, round_id) = {
        //info!("🔍 [DEBUG] Waiting to acquire node lock for Entering handle commit");
        let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for Entering handle commit");
        (node_guard.id, commit_request.round_id)
    };

    info!(
        "============== Node {}: Handling commit request from {} for round {} ==============",
        node_id, commit_request.proposing_node_id, round_id
    );

    // ✅ Exit early if the round is already finalized
    {
        //info!("🔍 [DEBUG] Waiting to acquire node lock for handle commit 1");
        let node_guard = node.lock().await;
//info!("🔓 [DEBUG] Acquired node lock for for handle commit 1");
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
        //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
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

    if commit_count == quorum_threshold {
        info!(
            "Node {}: Finalizing round {} with {}/{} commits.",
            node_id, round_id, commit_count, quorum_threshold
        );

        let all_commits;
        {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
            let mut commit_tracker = node_guard.commit_tracker.lock().await;
            all_commits = commit_tracker.remove(&round_id).unwrap_or_default();
        }

        let mut all_units = Vec::new();
        for commit in &all_commits {
            all_units.extend(commit.units.clone());
        }

        {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
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
            //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
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



        let total_rounds: u64;
        let round_id: u64;
        {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for Entering handle commit");
            let node_guard = node.lock().await;
            //info!("🔓 [DEBUG] Acquired node lock for Entering handle commit");
            round_id = commit_request.round_id;
            total_rounds = node_guard.total_rounds as u64;
        }

        if round_id >= total_rounds {
            info!("Node {}: Total Round {} finalized. Exiting commit handler Application DONE.", node_id,total_rounds);
            

            {
                let mut node_guard = node.lock().await;
            
                if node_guard.rbc_processor.is_some() {
                    info!("🛑 Terminating RBCProcessor...");
            
                    // ✅ Take the processor out of Node safely
                    let processor = node_guard.rbc_processor.take();
            
                    // ✅ Drop the lock before performing termination
                    drop(node_guard);
            
                    if let Some(processor) = processor {
                        info!("🗑️ Dropping RBCProcessor instance...");
                        drop(processor); // ✅ This actually kills it
                    }
            
                    info!("✅ RBCProcessor successfully terminated.");
                }
            }


            {
                //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
            let node_guard = node.lock().await;
            //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
                let mut current_round = node_guard.current_round.lock().await;
                if *current_round == round_id {
                    *current_round += 1;
                    info!(
                        "Node {}: Local round advanced to {}",
                        node_id, *current_round
                    );
                }
            }


            


            return Ok(());
        }

        // ✅ **Step 26:** Increment the round if applicable
        {
            //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
            let mut current_round = node_guard.current_round.lock().await;
            if *current_round == round_id {
                *current_round += 1;
                info!(
                    "Node {}: Local round advanced to {}",
                    node_id, *current_round
                );
            }
        }

        // ✅ Emit RoundFinalized Event when the round is finalized
        info!("Node {}: Enqueuing RoundFinalized event for round {}", node_id, round_id);
        let round_finalized_message = RBCMessage::RoundFinalized(round_id);

        // ✅ Acquire lock on `node` to access `rbc_processor`
        //info!("🔍 [DEBUG] Waiting to acquire node lock for round handle commit");
        let node_guard = node.lock().await;
        //info!("🔓 [DEBUG] Acquired node lock for round handle commit");
        if let Some(rbc_processor) = &node_guard.rbc_processor {
            rbc_processor.enqueue_message(round_finalized_message).await; // ✅ Call from `node`
        } else {
            error!("❌ Node {}: RBCProcessor not initialized when trying to enqueue RoundFinalized event!", node_id);
        }
    }
    info!(
        "============== Node {}: Exiting commit handler.==============",
        node_id
    );
    Ok(())
}
