use serde::{Deserialize, Serialize};

// Message types
#[derive(Deserialize)]
pub struct ProposeRequest {
pub sender: usize,
pub shard: Vec<u8>,
pub proof: Vec<Vec<u8>>,
pub root: Vec<u8>,
pub epoch_id: u64,
}

#[derive(Deserialize, Serialize)]
pub struct PrevoteRequest {
pub sender: usize,
pub root: Vec<u8>,
pub epoch_id: u64,
pub unit: Vec<u8>,
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
