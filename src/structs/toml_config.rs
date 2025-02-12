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
    pub ip_address: String,
    pub total_nodes: usize,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct ConsensusConfig {
    pub transaction_size: usize,
    pub data_shards: usize,
    pub number_of_transactions: usize,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct NodeConfig {
    pub id: usize,
}