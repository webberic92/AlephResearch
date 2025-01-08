use std::fs;

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