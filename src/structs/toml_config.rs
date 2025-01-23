use serde::Deserialize;

// Configuration structures
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TomlConfig {
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub node: NodeConfig,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct NetworkConfig {
    pub listen_address: String,
    pub nodes: Vec<String>,
    pub ip_manager_address: String, // Added for GTC APIs
    pub proposals: Vec<usize>,      // Add this line
    pub ip_address: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ConsensusConfig {
    pub transaction_size: usize,
    pub data_shards: usize,
    pub batch_size: usize,
    pub epoch_round_id: u64,

}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct NodeConfig {
    pub id: usize,
    pub total_nodes: usize,
}