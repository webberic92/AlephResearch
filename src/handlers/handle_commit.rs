use std::{process::Command, sync::{atomic::Ordering, Arc}};
use chrono::Local;
use tokio::sync::Mutex;
use tracing::{info, error};
use crate::{
    processors::priority_queue::RBCMessage,
    structs::{node::Node, requests::CommitRequest},
    utils::config_util::write_finalized_dag_to_file,
};

pub async fn handle_commit(
    node: Arc<Mutex<Node>>,
    commit_request: CommitRequest,
) -> Result<(), String> {
    let (node_id, round_id) = {
        let node_guard = node.lock().await;
        node_guard.message_count.fetch_add(1, Ordering::Relaxed);
        (node_guard.id, commit_request.round_id)
    };

    {
        let node_guard = node.lock().await;
        let dag_guard = node_guard.dag.lock().await;
        if dag_guard.contains_key(&round_id) {
            info!("Node {}: Commit for round {} already finalized. Ignoring duplicate.", node_id, round_id);
            return Ok(());
        }
    }

    let commit_count;
    let quorum_threshold;
    {
        let node_guard = node.lock().await;
        let mut commit_tracker = node_guard.commit_tracker.lock().await;

        let round_commits = commit_tracker.entry(round_id).or_insert_with(Vec::new);
        if round_commits.iter().any(|c| c.proposing_node_id == commit_request.proposing_node_id) {
            info!("Node {}: Duplicate commit from {} for round {}.", node_id, commit_request.proposing_node_id, round_id);
            return Ok(());
        }

        round_commits.push(commit_request.clone());
        commit_count = round_commits.len();
        quorum_threshold = node_guard.get_quorum_threshold();
    }

    info!("Node {}: Commit count for round {} is {}/{}.", node_id, round_id, commit_count, quorum_threshold);

    if commit_count == quorum_threshold {
        info!("Node {}: Finalizing round {} with quorum.", node_id, round_id);

        let all_commits = {
            let node_guard = node.lock().await;
            let mut commit_tracker = node_guard.commit_tracker.lock().await;
            commit_tracker.remove(&round_id).unwrap_or_default()
        };

        let mut all_units = Vec::new();
        for commit in &all_commits {
            all_units.extend(commit.units.clone());
        }

        {
            let node_guard = node.lock().await;
            let mut dag = node_guard.dag.lock().await;
            let dag_units = dag.entry(round_id).or_insert_with(Vec::new);
            info!("Node {}: Inserting {} units into DAG for round {}", node_id, all_units.len(), round_id);
            for unit in all_units {
                
                if !dag_units.iter().any(|u| u.merkle_root == unit.merkle_root) {
                    dag_units.push(unit.clone());
                    info!(
                        "Node {}: Inserted unit {} (creator: {}, tx count: {}) into DAG round {}",
                        node_id,
                        unit.unit_id,
                        unit.proposer_node,
                        unit.transactions.len(),
                        round_id
                    );
                }
            }
        }

        // ✅ Immediately emit RoundFinalized event before doing anything else
        {
            info!("Node {}: Enqueuing RoundFinalized event for round {}", node_id, round_id);
            let round_finalized = RBCMessage::RoundFinalized(round_id);
            let node_guard = node.lock().await;
            if let Some(rbc_processor) = &node_guard.rbc_processor {
                rbc_processor.enqueue_message(round_finalized).await;
            } else {
                error!("❌ Node {}: No RBCProcessor to enqueue RoundFinalized!", node_id);
            }
        }

        let finalized_dag = {
            let node_guard = node.lock().await;
            let dag_guard = node_guard.dag.lock().await;
            dag_guard.clone()
        };

        if let Err(e) = write_finalized_dag_to_file(
            "/aleph/finalized_dag",
            &finalized_dag,
            round_id,
        )
        .await
        {
            error!("Node {}: Failed to write finalized DAG: {:?}", node_id, e);
            return Err(format!("DAG write failed: {:?}", e));
        }

        let message_count = node.lock().await.message_count.clone();
        info!("Node {}: Finalized round {}. COMMUNICATION OVERHEAD {:?}", node_id, round_id, message_count);
        info!(
            "Node {}: Finalized round {} with {}/{} commits. USE THIS FOR TPS METRIC",
            node_id, round_id, commit_count, quorum_threshold
        );
        // ✅ Update round locally
        {
            let node_guard = node.lock().await;
            let mut current_round = node_guard.current_round.lock().await;
            if *current_round == round_id {
                *current_round += 1;
                info!("Node {}: Local round advanced to {}", node_id, *current_round);
            }
        }

        // ✅ Final round: log, upload, terminate
        {
            let total_rounds = {
                let node_guard = node.lock().await;
                node_guard.total_rounds as u64
            };

            if round_id >= total_rounds {
                info!("Node {}: Round {} was final. Shutting down.", node_id, round_id);
                info!("LATENCY END: {}", Local::now().format("%Y-%m-%d %H:%M:%S"));

                let (instances, txs, rounds, node_id) = {
                    let node_guard = node.lock().await;
                    (
                        node_guard.total_nodes,
                        node_guard.number_of_transactions,
                        node_guard.total_rounds,
                        node_guard.id,
                    )
                };

                let s3_upload_cmd = format!(
                    r#"(S3_FOLDER="logs/ORIG_N{instances}_T{txs}_R{rounds}/node-{node_id}" && \
                    aws s3 cp /aleph/logs/ s3://aleph-research/$S3_FOLDER/ --recursive --quiet) &"#,
                );

                tokio::spawn(async move {
                    match Command::new("sh").arg("-c").arg(&s3_upload_cmd).spawn() {
                        Ok(_) => info!("✅ S3 upload triggered."),
                        Err(e) => error!("❌ S3 upload command failed: {:?}", e),
                    }
                });

                {
                    let mut node_guard = node.lock().await;
                    if let Some(processor) = node_guard.rbc_processor.take() {
                        drop(processor);
                        info!("🛑 RBCProcessor terminated.");
                    }
                }
            }
        }
    }

    Ok(())
}