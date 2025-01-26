use aleph_research::requests::send_prevotes::send_prevotes;
use aleph_research::structs::toml_config::TomlConfig;
use aleph_research::utils::create_transaction_data::create_transaction_data;
use reqwest::Client;
use tracing::{error, info};
use tracing_subscriber;

// Utility imports for configuration and network operations
use aleph_research::utils::config_util::{
    are_enough_proposals_received, load_config, update_proposals_in_config,
};
use aleph_research::requests::send_proposals::send_proposals;
use aleph_research::utils::start_util::{wait_for_all_nodes_health, wait_for_turn};

use aleph_research::requests::ip_server_requests::notify_transaction_submitted;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let toml_config: TomlConfig = load_config();
    let client = Client::new();
    wait_for_all_nodes_health(&client, &toml_config).await;
    wait_for_turn(&client, &toml_config).await?;

    // Generate transaction data and handle the result
    match create_transaction_data(&toml_config) {
        Ok((shards, merkle_root)) => {
            // info!("Transaction data created successfully.");

            // Send proposals
            send_proposals(&client, &toml_config, &shards, &merkle_root).await?;

            // Update the proposals field in the configuration file
            let updated_toml_config =update_proposals_in_config()?;

            // Check if enough proposals have been received to move to the next phase
            if are_enough_proposals_received().await {
                info!(
                    "Node {}: sending prevote in epoch {} from handle start",
                 updated_toml_config.node.id, updated_toml_config.consensus.epoch_round_id
                );
                // Logic for sending prevotes is commented out for now
                send_prevotes(&client, &updated_toml_config, &merkle_root, &shards).await?;
            }else{
                
                //might need else here but notify transaction also called in commit
            // Notify that the transaction has been submitted (optional, currently commented out)
             notify_transaction_submitted(&client, &toml_config).await?;
            }

        }
        Err(e) => {
            error!("Failed to create transaction data: {}", e);
            return Err(e); // Exit early on error
        }
    }

    // Indicate successful execution
    Ok(())
}
