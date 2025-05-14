use serde_json::json;
use sha2::Sha256;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::info;
use std::collections::HashMap;
use std::path::Path;
use base64::{engine::general_purpose, Engine};
use crate::structs::requests::DagUnit;
use crate::structs::toml_config::TomlConfig;
use sha2::Digest;

/// Load configuration
pub fn load_config(path: Option<&str>) -> TomlConfig {
    let config_path = path.unwrap_or("/aleph/aleph-node-config.toml");
    let config_contents = std::fs::read_to_string(config_path).expect("Failed to read configuration file.");
    toml::from_str(&config_contents).expect("Failed to parse configuration.")
}

pub fn save_config(toml_config: &TomlConfig, path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = path.unwrap_or("/aleph/aleph-node-config.toml");
    let config_contents = toml::to_string(&toml_config)
        .expect("Failed to serialize configuration.");
    std::fs::write(config_path, config_contents)
        .expect("Failed to write configuration file.");
    Ok(())
}

/// **Writes the finalized DAG to a file in a human-readable format.**
pub async fn write_finalized_dag_to_file(
    base_path: &str,
    dag: &HashMap<u64, Vec<DagUnit>>,
    round_id: u64,  
) -> Result<(), Box<dyn std::error::Error>> {
    if dag.is_empty() {
        return Ok(()); // No units to write
    }

    if let Some(units) = dag.get(&round_id) {
        let round_file = format!("{}/round{}.json", base_path, round_id);
        let path = Path::new(&round_file);

        if let Some(parent_dir) = path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir).await?;
            }
        }

        let round_data: Vec<_> = units.iter().map(|unit| json!({
            "unit_id": unit.unit_id,
            "creator": unit.proposer_node,
            "round": unit.round,
            "batch_merkle_root": hex::encode(&unit.merkle_root),
            "transactions": unit.transactions.iter().map(|tx| json!({
                "hash": hex::encode(&tx.root),  // renamed for clarity
                "proofs": tx.proofs.clone(),
                "shards": tx.shards.iter().map(|shard| {
                    let decoded_bytes = general_purpose::STANDARD.decode(shard)
                        .unwrap_or_else(|_| vec![]);
                    if !decoded_bytes.is_empty() {
                        String::from_utf8(decoded_bytes.clone())
                            .unwrap_or_else(|_| format!("{:?}", decoded_bytes))
                    } else {
                        shard.clone()
                    }
                }).collect::<Vec<String>>(),
            })).collect::<Vec<_>>(),
            "parent_hashes": unit.parent_units.iter().map(|p| {
                let hash = Sha256::digest(p.as_bytes());
                hex::encode(hash)
            }).collect::<Vec<_>>(), 
           "finalization_timestamp": unit.finalization_timestamp,
        })).collect();
        

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
