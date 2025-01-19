use serde::{Deserialize, Serialize};

#[derive(Deserialize,Debug)]
pub struct ProposeRequest {
    pub sender: usize,               // ID of the node sending the proposal
    pub shards: Vec<Vec<u8>>,        // Data shards for the proposal
    pub proofs: Vec<Vec<Vec<u8>>>,   // Merkle proofs for each shard
    pub root: Vec<u8>,               // Merkle root of the tree
    pub epoch_id: u64,               // Epoch ID for the proposal
}

#[derive(Serialize,Deserialize)]
pub struct PrevoteRequest {
    pub sender: usize,
    pub root: Vec<u8>,
    pub proofs: Vec<Vec<Vec<u8>>>,
    pub shards: Vec<Vec<u8>>,
    pub epoch_id: u64,
    pub node_url: String,
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
    pub sender: String,
}

