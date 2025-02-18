use crate::structs::requests::DagUnit;
use crate::structs::toml_config::TomlConfig;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use base64::engine::general_purpose;

use base64::Engine;
use serde_json::Value;
use tokio::fs::{ self, OpenOptions };
use tokio::io::AsyncWriteExt;
use tracing::{error, info};
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


// **Writes the finalized unit to the round file**
pub async fn write_finalized_dag_to_file(
    base_path: &str,
    dag: &HashMap<u64, Vec<DagUnit>>,
    round_id: u64,  // ✅ Only write finalized units for this round
) -> Result<(), Box<dyn std::error::Error>> {
    if dag.is_empty() {
        error!("DAG is empty, nothing to write.");
        return Ok(()); 
    }

    // ✅ Fetch only the finalized units for the given round
    if let Some(units) = dag.get(&round_id) {
        let round_file = format!("{}/round{}.json", base_path, round_id);
        let path = Path::new(&round_file);

        if let Some(parent_dir) = path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
            }
        }

        let round_data: Vec<Value> = units
            .iter()
            .map(|unit| serde_json::json!({
                "unit_id": unit.unit_id,
                "creator": unit.proposer_node,
                "round": unit.round,
                "transactions": unit.transactions.iter().map(|tx| {
                    serde_json::json!({
                        "tx_id": tx.tx_id,
                        "data": &tx.data,
                    })
                }).collect::<Vec<Value>>(),
                "parents": unit.parent_units,
                "merkle_root": unit.merkle_root,
                "finalization_timestamp": unit.finalization_timestamp,
            }))
            .collect();

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&round_file)
            .await?;

        file.write_all(serde_json::to_string_pretty(&round_data)?.as_bytes()).await?;

        info!("Successfully wrote finalized DAG for round {} to file: {}", round_id, round_file);
    }

    Ok(())
}


