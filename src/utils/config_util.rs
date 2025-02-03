
use tracing::{error, info};

use crate::structs::toml_config::TomlConfig;

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
