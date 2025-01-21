use aleph_research::structs::toml_config::TomlConfig;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use tracing_subscriber;

// Utility imports for configuration and network operations
use aleph_research::utils::config_util::{
    are_enough_proposals_received, load_config, save_config, update_proposals_in_config,
};
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::start_util::{wait_for_all_nodes_health, wait_for_turn};
use aleph_research::utils::merkle_utils::{
    compute_merkle_root, split_into_shards, validate_shard_sizes,
};
use aleph_research::requests::ip_server_requests::notify_transaction_submitted;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let toml_config: TomlConfig = load_config("/home/aleph-node/aleph-node-config.toml");
    let client = Client::new();
    wait_for_all_nodes_health(&client, &toml_config).await;
    wait_for_turn(&client, &toml_config).await?;

    // Generate deterministic transaction data of the specified size
    let transaction_data = vec![1; toml_config.consensus.transaction_size];
    info!(
        "Generated transaction data of size: {} bytes",
        transaction_data.len()
    );

    // Split the transaction data into shards for distribution
    let shards = split_into_shards(&transaction_data, toml_config.consensus.data_shards);
    info!(
        "Transaction data split into {} shards",
        toml_config.consensus.data_shards
    );

    // Compute cryptographic proofs (hashes) for each shard
    let proofs: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| {
            let hash = Sha256::digest(shard).to_vec();
            info!("Computed hash for shard: {:?}", hash);
            hash
        })
        .collect();

    // Validate that shard sizes conform to protocol constraints
    if let Err(e) = validate_shard_sizes(&shards, toml_config.consensus.transaction_size) {
        error!("Shard Size Validation failed: {}", e);
        return Err(e.into());
    }
    
    // Compute the Merkle root for the set of shard proofs
    let merkle_root = compute_merkle_root(&proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    // Send proposals (shards, proofs, and Merkle root) to other nodes in the network
    send_proposals(&client, &toml_config, &shards, &proofs, &merkle_root).await?;

    // Update the proposals field in the configuration file
    update_proposals_in_config("/home/aleph-node/aleph-node-config.toml")?;

    // Check if enough proposals have been received to move to the next phase
    if are_enough_proposals_received().await {
        // Logic for sending prevotes is commented out for now
        // send_prevotes(&client, &updated_toml_config, &merkle_root, &proofs, &shards).await?;
    }

    // Notify that the transaction has been submitted (optional, currently commented out)
    // notify_transaction_submitted(&client, &updated_toml_config).await?;

    // Indicate successful execution
    Ok(())
}
