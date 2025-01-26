use std::fs;

use tracing::{error, info};

use crate::structs::{node::Node, toml_config::TomlConfig};

/// Load configuration
pub fn load_config() -> TomlConfig {
    let config_contents = fs::read_to_string("/home/aleph-node/aleph-node-config.toml").expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}
pub fn save_config(toml_config: &TomlConfig) -> Result<(), Box<dyn std::error::Error>> {
    let config_contents = toml::to_string(&toml_config)
        .expect("Failed to serialize configuration.");
    fs::write("/home/aleph-node/aleph-node-config.toml", config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}

// Check if all proposals have been received
// ch-RBC proof: Ensures a majority quorum (2f+1) of proposals before moving to prevote.
// Check if a majority quorum (2f + 1) of proposals has been received
pub async fn are_enough_proposals_received() -> bool {
    let config = load_config();
    info!("Node {} {}: CHECKING IF ALL PROPOSALS RECEIVED total nodes == {}", config.node.id,config.network.ip_address,config.node.total_nodes);

    let faulty_nodes = (config.node.total_nodes - 1) / 3; // f = ⌊(N-1)/3⌋
    info!("Node {} {}: FAULTY NODES ALLOWED == {}", config.node.id,config.network.ip_address,faulty_nodes);

    let required_quorum = 2 * faulty_nodes + 1; //2F+1
    info!("Node {} {}: required_quorum == {}", config.node.id,config.network.ip_address,required_quorum);
    info!("Node {} {}: is proposals length {} >= required_quorum {}", config.node.id,config.network.ip_address,config.network.proposals.len(),required_quorum);

    if(config.network.proposals.len() >= required_quorum){
        info!(
            "Node {} {}: Enough proposals. received to start prevoting form aleph_start. Proposals array = {:?}",
            config.node.id, config.network.ip_address, config.network.proposals
        );
        return true
    }else{
        return false
    }
}

pub fn update_proposals_in_config() -> Result<TomlConfig, Box<dyn std::error::Error>> {
    let mut updated_toml_config = load_config();
    if !updated_toml_config.network.proposals.contains(&updated_toml_config.node.id) {
        updated_toml_config.network.proposals.push(updated_toml_config.node.id);
        save_config(&updated_toml_config)?;
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
pub fn persist_proposal_tracker(proposal_tracker: &Vec<usize>, config_path: &str) {
    let mut config = load_config();
    config.network.proposals = proposal_tracker.clone();

    match save_config(&config) {
        Ok(_) => info!("Node {} {} Successfully updated proposal tracker in toml.", config.node.id, config.network.ip_address),
        Err(e) => error!("Node {} {} Failed to update proposal tracker. Error: {:?}", config.node.id, config.network.ip_address, e),
    }
}

// Update the proposal tracker
// Tracks which nodes have submitted valid proposals to ensure quorum.
pub async fn update_proposal_tracker(node: &Node, sender: usize, epoch_id: u64) {
    let config_path = "/home/aleph-node/aleph-node-config.toml";
    let config = load_config();
    let mut proposal_tracker = config.network.proposals.clone();
    proposal_tracker.push(sender);
    persist_proposal_tracker( &proposal_tracker, config_path);
    info!(
        "Node {} {}  HANDLE PROPOSE: Updated proposal tracker for epoch {}: {:?}",
        node.id, node.ip_address, epoch_id, proposal_tracker
    );
}

// Update the epoch tracker
// ch-RBC proof: Keeps track of the current epoch and ensures nodes stay synchronized.
async fn update_epoch_tracker(node: &Node, epoch_id: u64) {
    let mut epoch_tracker = node.epoch_round_id.lock().await;
    epoch_tracker.insert(epoch_id + 1);
    persist_epoch_round_id( epoch_id, "/home/aleph-node/aleph-node-config.toml").await;
}

// Persist the epoch round ID to the TOML file
pub async fn persist_epoch_round_id(epoch_id: u64, config_path: &str) {
    let mut config = load_config();
    config.consensus.epoch_round_id = epoch_id + 1;

    match save_config( &config) {
        Ok(_) => info!("Node {} {} Successfully updated epoch_round_id = {}.", config.node.id, config.network.ip_address, config.consensus.epoch_round_id),
        Err(e) => error!("Node {} {} Failed to update epoch_round_id. Error: {:?}", config.node.id, config.network.ip_address, e),
    }
}