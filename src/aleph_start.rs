
use aleph_research::structs::toml_config::TomlConfig;
use reqwest::Client;
use sha2::{Digest, Sha256};

use tracing::{error, info};
use tracing_subscriber;
use aleph_research::utils::config_util::{are_enough_proposals_received, load_config, save_config};
// use aleph_research::utils::ip_server_utils:: notify_transaction_submitted;
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::rbc_utils::{wait_for_all_nodes_health, wait_for_turn};
use aleph_research::utils::merkle_utils::compute_merkle_root;
use aleph_research::requests::ip_server_requests::notify_transaction_submitted;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();

    let toml_config: TomlConfig = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();

    wait_for_all_nodes_health(&client, &toml_config).await;
    wait_for_turn(&client, &toml_config).await?;

    let transaction_data = vec![1; toml_config.consensus.transaction_size];
    let shards = split_into_shards(&transaction_data, toml_config.consensus.data_shards);
    let (proofs, merkle_root) = compute_proofs_and_merkle_root(&shards);

    // Validate shard size according to protocol constraints
    for shard in &shards {
        let is_valid = check_shard_size(shard.len(), toml_config.consensus.batch_size);
        if !is_valid {
            error!(
                "Shard size validation failed. Size: {}, Batch size limit: {}",
                shard.len(),
                toml_config.consensus.batch_size
            );
        }
    }

    send_proposals(&client, &toml_config, &shards, &proofs, &merkle_root).await?;
    let updated_toml_config = update_proposals_in_config("/home/aleph-node/aleph-node-config.toml")?;

    if are_enough_proposals_received().await {
        // send_prevotes(&client, &updated_toml_config, &merkle_root, &proofs, &shards).await?;
    }

    // notify_transaction_submitted(&client, &updated_toml_config).await?;
    Ok(())
}



/// Splits transaction data into shards
fn split_into_shards(transaction_data: &[u8], data_shards: usize) -> Vec<Vec<u8>> {
    let shard_size = transaction_data.len() / data_shards;
    let shards: Vec<Vec<u8>> = transaction_data
        .chunks(shard_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    info!("Transaction data size: {}", transaction_data.len());
    info!(
        "Shard sizes: {:?}",
        shards.iter().map(|s| s.len()).collect::<Vec<_>>()
    );
    info!(
        "Total size of all shards: {}",
        shards.iter().map(|s| s.len()).sum::<usize>()
    );
    assert_eq!(
        shards.len(),
        data_shards,
        "Shard count mismatch: expected {}, found {}",
        data_shards,
        shards.len()
    );

    for (i, shard) in shards.iter().enumerate() {
        info!(
            "Shard {}: Size = {}, Data = {:?}",
            i,
            shard.len(),
            &shard[0..std::cmp::min(10, shard.len())] // Log only the first 10 bytes for readability
        );
    }

    shards
}

/// Computes Merkle proofs and root from shards
fn compute_proofs_and_merkle_root(shards: &[Vec<u8>]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let proofs: Vec<Vec<u8>> = shards.iter().map(|s| {
        let hash = Sha256::digest(s).to_vec();
        info!("Computed hash for shard: {:?}", hash);
        hash
    }).collect();

    let merkle_root = compute_merkle_root(&proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    (proofs, merkle_root)
}


/// Check shard size validity according to protocol constraints
fn check_shard_size(shard_size: usize, batch_size_limit: usize) -> bool {
    shard_size <= batch_size_limit
}


pub fn update_proposals_in_config(config_path: &str) -> Result<TomlConfig, Box<dyn std::error::Error>> {
    let mut updated_toml_config = load_config(config_path);
    if !updated_toml_config.network.proposals.contains(&updated_toml_config.node.id) {
        updated_toml_config.network.proposals.push(updated_toml_config.node.id);
        save_config(config_path, &updated_toml_config)?;
        info!(
            "Node {} {}: Added to proposals. Current proposals: {:?}",
            updated_toml_config.node.id,
            updated_toml_config.network.ip_address,
            updated_toml_config.network.proposals
        );
    }
    Ok(updated_toml_config)
}