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

    let (commit_count, quorum_threshold) = {
        let node_guard = node.lock().await;
        let mut commit_tracker = node_guard.commit_tracker.lock().await;

        let round_commits = commit_tracker.entry(round_id).or_insert_with(Vec::new);
        if round_commits.iter().any(|c| c.proposing_node_id == commit_request.proposing_node_id) {
            info!("Node {}: Duplicate commit from {} for round {}.", node_id, commit_request.proposing_node_id, round_id);
            return Ok(());
        }

        round_commits.push(commit_request.clone());
        (round_commits.len(), node_guard.get_quorum_threshold())
    };

    info!("Node {}: Commit count for round {} is {}/{}.", node_id, round_id, commit_count, quorum_threshold);

    if commit_count == quorum_threshold {
        info!("Node {}: Finalizing round {} with quorum.", node_id, round_id);

        let all_commits = {
            let node_guard = node.lock().await;
            let mut commit_tracker = node_guard.commit_tracker.lock().await;
            commit_tracker.remove(&round_id).unwrap_or_default()
        };

        let mut unique_units = Vec::new();
        let mut seen_unit_ids = std::collections::HashSet::new();

        for commit in &all_commits {
            for mut unit in commit.units.clone() {
                if seen_unit_ids.insert(unit.unit_id.clone()) {
                    // ✅ FINAL PATCH: populate accumulator_root
                    if let Some(first_tx) = unit.transactions.get(0) {
                        unit.accumulator_root = first_tx.accumulator.clone().into_bytes();
                        info!(
                            "Node {}: Inserted unit {} with accumulator_root = {}",
                            node_id, unit.unit_id, first_tx.accumulator
                        );
                    }
                    unique_units.push(unit);
                } else {
                    info!("Node {}: Skipping duplicate unit {} during DAG insertion", node_id, unit.unit_id);
                }
            }
        }

        {
            let node_guard = node.lock().await;
            let mut dag = node_guard.dag.lock().await;
            let dag_units = dag.entry(round_id).or_insert_with(Vec::new);
            info!("Node {}: Inserting {} unique units into DAG for round {}", node_id, unique_units.len(), round_id);
            for unit in unique_units {
                dag_units.push(unit.clone());
                info!(
                    "Node {}: Inserted unit {} (creator: {}, tx count: {}) into DAG round {}",
                    node_id, unit.unit_id, unit.proposer_node, unit.transactions.len(), round_id
                );
            }
        }

        let finalized_dag = {
            let node_guard = node.lock().await;
            let dag_guard = node_guard.dag.lock().await;
            dag_guard.clone()
        };

        {
            let node_guard = node.lock().await;
            node_guard.hash_to_prime_cache.lock().await.remove(&round_id);
        }

        if let Err(e) = write_finalized_dag_to_file("/aleph/finalized_dag", &finalized_dag, round_id).await {
            error!("Node {}: Failed to write finalized DAG: {:?}", node_id, e);
            return Err(format!("DAG write failed: {:?}", e));
        }

        {
            let node_guard = node.lock().await;
            let message_count = node_guard.message_count.clone();
            info!("Node {}: Finalized round {}. COMMUNICATION OVERHEAD {:?}", node_id, round_id, message_count);
            info!("Node {}: Finalized round {} with {}/{} commits. USE THIS FOR TPS METRIC", node_id, round_id, commit_count, quorum_threshold);
        }

        {
            let node_guard = node.lock().await;
            let mut current_round = node_guard.current_round.lock().await;
            if *current_round == round_id {
                *current_round += 1;
                info!("Node {}: Local round advanced to {}", node_id, *current_round);
            }
        }

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

        {
            let (instances, txs, rounds, id,instance_type) = {
                let node_guard = node.lock().await;
                (
                    node_guard.total_nodes,
                    node_guard.number_of_transactions,
                    node_guard.total_rounds,
                    node_guard.id,
                    node_guard.instance_type.clone(),

                )
            };

            if round_id >= rounds as u64 {
                info!("Node {}: Round {} was final. Shutting down.", node_id, round_id);
                info!("LATENCY END: {}", Local::now().format("%Y-%m-%d %H:%M:%S"));

                let s3_upload_cmd = format!(
                    r#"(S3_FOLDER="logs/{instance_type}_ORIG_N{instances}_T{txs}_R{rounds}/node-{id}" && \
                    aws s3 cp /aleph/logs/ s3://aleph-research/$S3_FOLDER/ --recursive --quiet) &"#,
                );

                tokio::spawn(async move {
                    match Command::new("sh").arg("-c").arg(&s3_upload_cmd).spawn() {
                        Ok(_) => info!("✅ S3 upload triggered."),
                        Err(e) => error!("❌ S3 upload command failed: {:?}", e),
                    }
                });

                let mut node_guard = node.lock().await;
                if let Some(processor) = node_guard.rbc_processor.take() {
                    drop(processor);
                    info!("🛑 RBCProcessor terminated.");
                }
            }
        }
    }

    Ok(())
}

