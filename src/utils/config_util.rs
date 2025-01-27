use std::fs;

use tracing::{error, info};

use crate::structs::{node::Node, toml_config::TomlConfig};

/// Load configuration
pub fn load_config(path: Option<&str>) -> TomlConfig {
    let config_path = path.unwrap_or("/home/aleph-node/aleph-node-config.toml");
    let config_contents = std::fs::read_to_string(config_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

pub fn save_config(toml_config: &TomlConfig, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = path.unwrap_or("/home/aleph-node/aleph-node-config.toml");
    let config_contents = toml::to_string(&toml_config)
        .expect("Failed to serialize configuration.");
    std::fs::write(config_path, config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}

// Check if all proposals have been received
// ch-RBC proof: Ensures a majority quorum (2f+1) of proposals before moving to prevote.
// Check if a majority quorum (2f + 1) of proposals has been received
pub async fn are_enough_proposals_received() -> bool {
    let config = load_config( None);
    info!("Node {} {}: CHECKING IF ALL PROPOSALS RECEIVED total nodes == {}", config.node.id,config.network.ip_address,config.node.total_nodes);

    let faulty_nodes = (config.node.total_nodes - 1) / 3; // f = ⌊(N-1)/3⌋
    info!("Node {} {}: FAULTY NODES ALLOWED == {}", config.node.id,config.network.ip_address,faulty_nodes);

    let required_quorum = 2 * faulty_nodes + 1; //2F+1
    info!("Node {} {}: required_quorum == {}", config.node.id,config.network.ip_address,required_quorum);
    info!("Node {} {}: is proposals length {} >= required_quorum {}", config.node.id,config.network.ip_address,config.network.proposals.len(),required_quorum);

    if(config.network.proposals.len() >= required_quorum){
        info!(
            "Node {} {}: Enough proposals. received to start prevoting. Proposals array = {:?}",
            config.node.id, config.network.ip_address, config.network.proposals
        );
        return true
    }else{
        return false
    }
}

pub fn update_proposals_in_config() -> Result<TomlConfig, Box<dyn std::error::Error>> {
    let mut updated_toml_config = load_config( None);
    if !updated_toml_config.network.proposals.contains(&updated_toml_config.node.id) {
        updated_toml_config.network.proposals.push(updated_toml_config.node.id);
        save_config(&updated_toml_config, None )?;
        info!(
            "Node {} {}: Added to proposals. Current proposals: {:?}",
            updated_toml_config.node.id,
            updated_toml_config.network.ip_address,
            updated_toml_config.network.proposals
        );
    }
    Ok(updated_toml_config)
}

// Persist the proposal tracker to the TOML file
pub fn persist_proposal_tracker(proposal_tracker: &Vec<usize>) {
    let mut config = load_config(None);
    config.network.proposals = proposal_tracker.clone();

    match save_config(&config,None) {
        Ok(_) => info!("Node {} {} Successfully updated proposal tracker in toml.", config.node.id, config.network.ip_address),
        Err(e) => error!("Node {} {} Failed to update proposal tracker. Error: {:?}", config.node.id, config.network.ip_address, e),
    }
}

// Update the proposal tracker
// Tracks which nodes have submitted valid proposals to ensure quorum.
pub async fn update_proposal_tracker(node: &Node, sender: usize, epoch_id: u64) {
    let config = load_config(None);
    let mut proposal_tracker = config.network.proposals.clone();
    proposal_tracker.push(sender);
    persist_proposal_tracker( &proposal_tracker);
    info!(
        "Node {} {} Updated proposal tracker for epoch {}: node ID list {:?}",
        node.id, node.ip_address, epoch_id, proposal_tracker
    );
}


// Persist the epoch round ID to the TOML file
pub async fn persist_epoch_round_id(epoch_id: u64) -> Result<(), std::io::Error> {
    // Load the current TOML configuration
    let mut config = load_config(None);

    // Update the epoch ID
    config.consensus.epoch_round_id = epoch_id;

    // Save the updated configuration back to the TOML file
    let _ = save_config(&config,None);

    info!("Persisted updated epoch ID {} to TOML file.", epoch_id);
    Ok(())
}