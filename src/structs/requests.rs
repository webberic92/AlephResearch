use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct BaseRequest {
    pub proposing_node_id: usize,  // ID of the node sending the message
    pub root: Vec<u8>,     // Merkle root of the tree
    pub round_id: u64,     // Epoch ID
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
    pub unit: Vec<u8>,           // Reconstructed unit
    pub proofs: Vec<Vec<String>>,
    pub parents: Vec<String> // Reuse Merkle proofs from ProposeRequest
}


#[derive(Deserialize,Serialize)]
pub struct SyncEpochRequest {
pub round_id: u64,
pub sender: usize,
}

/// Represents a request for DAG synchronization.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DAGSyncRequest {
    pub round_id: u64,
    pub sender_id: usize,
    pub sender_url: String,
}

