use std::fs;

use tracing::info;

use crate::structs::toml_config::TomlConfig;

/// Load configuration
pub fn load_config(file_path: &str) -> TomlConfig {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}
pub fn save_config(file_path: &str, toml_config: &TomlConfig) -> Result<(), Box<dyn std::error::Error>> {
    let config_contents = toml::to_string(&toml_config)
        .expect("Failed to serialize configuration.");
    fs::write(file_path, config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}

// Check if all proposals have been received
// ch-RBC proof: Ensures a majority quorum (2f+1) of proposals before moving to prevote.
// Check if a majority quorum (2f + 1) of proposals has been received
pub async fn are_enough_proposals_received() -> bool {
    let config = load_config("/home/aleph-node/aleph-node-config.toml");
    info!("Node {} {}: CHECKING IF ALL PROPOSALS RECEIVED total nodes == {}", config.node.id,config.network.ip_address,config.node.total_nodes);

    let faulty_nodes = (config.node.total_nodes - 1) / 3; // f = ⌊(N-1)/3⌋
    info!("Node {} {}: FAULTY NODES ALLOWED == {}", config.node.id,config.network.ip_address,faulty_nodes);

    let required_quorum = 2 * faulty_nodes + 1; //2F+1
    info!("Node {} {}: required_quorum == {}", config.node.id,config.network.ip_address,required_quorum);
    info!("Node {} {}: is proposals length {} >= required_quorum {}", config.node.id,config.network.ip_address,config.network.proposals.len(),required_quorum);

    if(config.network.proposals.len() >= required_quorum){
        info!(
            "Node {} {}: Enough proposals. received to start prevoting form aleph_start.{:?}",
            config.node.id, config.network.ip_address, config.network.proposals
        );
        return true
    }else{
        return false
    }
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