use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProposeRequest {
    pub senderId: usize,               // ID of the node sending the proposal
    pub root: Vec<u8>,               // Merkle root of the tree
    pub proofs: Vec<Vec<String>>,    // Base64-encoded Merkle proofs for each shard
    pub shards: Vec<String>,         // Base64-encoded data shards for the proposal
    pub epoch_id: u64,               // Epoch ID for the proposal
}


#[derive(Serialize, Deserialize, Debug)]
pub struct PrevoteRequest {
    pub senderId: usize,
    pub root: Vec<u8>,               // Merkle root of the tree
    pub proofs: Vec<Vec<String>>,    // Base64-encoded Merkle proofs for each shard
    pub shards: Vec<String>,   
    pub epoch_id: u64,
    pub senderUrl: String
}

#[derive(Deserialize)]
pub struct CommitRequest {
pub sender: usize,
pub root: Vec<u8>,
pub epoch_id: u64,
pub unit: Vec<u8>,
}

#[derive(Deserialize)]
pub struct SyncEpochRequest {
pub epoch_id: u64,
pub sender: usize,
}

/// Represents a request for DAG synchronization.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DAGSyncRequest {
    /// The epoch ID for which DAG synchronization is requested.
    pub epoch_id: u64,
    
    /// The ID of the node making the request.
    pub senderId: usize,
    pub senderUrl: String,
}

