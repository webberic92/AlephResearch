use std::fs;

use crate::aleph_start::structs::Config;

/// Load configuration
pub fn load_config(file_path: &str) -> Config {
    let config_contents = fs::read_to_string(file_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}
pub fn save_config(file_path: &str, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_contents = toml::to_string(&config)
        .expect("Failed to serialize configuration.");
    fs::write(file_path, config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}