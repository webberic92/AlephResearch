use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRequest {
    pub proposing_node_id: u8,
    pub round_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub accumulator: String, // Base64-encoded accumulator (instead of Merkle root)
    pub proofs: Vec<Vec<String>>, // RSA inclusion proofs (per shard)
    pub shards: Vec<String>,      // Shards as base64 strings
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRequest {
    pub base: BaseRequest,
    pub transactions: Vec<Transaction>, // Array of transactions in a single proposal
    pub parents: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct PrevoteRequest {
    pub proposals: Vec<ProposeRequest>,  // ✅ Store multiple proposals in one struct
    pub sender_url: String,  // ✅ Preserve sender info
}



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitRequest {
    pub units: Vec<DagUnit>, 
    pub proposing_node_id: usize,  // ID of the node sending the message
    pub round_id: u64, 
}

#[derive(Deserialize,Serialize)]
pub struct SyncroundRequest {
pub round_id: u64,
pub sender: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagUnit {
    pub unit_id: String,
    pub proposer_node: usize,
    pub round: u64,
    pub transactions: Vec<Transaction>, // ✅ Store multiple transactions
    pub parent_units: Vec<String>,
    pub accumulator_root: Vec<u8>, // ✅ Store Merkle root as bytes
    pub finalization_timestamp: u64,
}


/// Represents a request for DAG synchronization.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DAGSyncRequest {
    pub round_id: u64,
    pub sender_id: usize,
    pub sender_url: String,
}

