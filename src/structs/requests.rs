use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct BaseRequest {
    pub proposing_node_id: usize,  // ID of the node sending the message
    pub root: Vec<u8>,     // Merkle root of the tree
    pub round_id: u64,     // round ID
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct ProposeRequest {
    pub base: BaseRequest,            // Base request fields
    pub proofs: Vec<Vec<String>>,     // Base64-encoded Merkle proofs for each shard
    pub shards: Vec<String>,    
    pub parents: Vec<String>,         // Base64-encoded parent hashes      // Base64-encoded data shards
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct PrevoteRequest {
    pub propose: ProposeRequest,      // Embed the ProposeRequest
    pub sender_url: String,           // URL of the sender
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommitRequest {
    pub base: BaseRequest,       // Base fields common to all phases
    pub units: Vec<DagUnit>,     // List of reconstructed units
    pub proofs: Vec<Vec<String>>, // Merkle proofs
    pub parents: Vec<String>,    // Parent units
}

#[derive(Deserialize,Serialize)]
pub struct SyncroundRequest {
pub round_id: u64,
pub sender: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub tx_id: String,
    pub data: Vec<u8>, // Transaction payload
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagUnit {
    pub unit_id: String,
    pub proposer_node: usize,
    pub round: u64,
    pub transactions: Vec<Transaction>, // ✅ Store multiple transactions
    pub parent_units: Vec<String>,
    pub merkle_root: String,
    pub finalization_timestamp: u64,
}


/// Represents a request for DAG synchronization.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DAGSyncRequest {
    pub round_id: u64,
    pub sender_id: usize,
    pub sender_url: String,
}

