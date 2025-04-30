use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRequest {
    pub proposing_node_id: u8,
    pub round_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardWithProofs {
    pub shard_b64: String,
    pub proofs: Vec<String>, // each entry is a round-specific proof
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub root: Vec<u8>,
    pub shards: Vec<ShardWithProofs>,
    pub accumulator: Option<String>, // ✅ NEW
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRequest {
    pub base: BaseRequest,
    pub transactions: Vec<Transaction>,
    pub parents: Vec<Vec<u8>>,
    pub batch_accumulator: String,               // 🆕 base64-encoded RSA accumulator
}



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrevoteRequest {
    pub proposals: Vec<ProposeRequest>,
    pub sender_url: String,
    pub sender_id: usize,  // NEW
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
    pub transactions: Vec<Transaction>,
    pub parent_units: Vec<String>, // ["U1-1", "U1-2"]
    pub accumulator_root: Vec<u8>,       // ✅ Single batch root
    pub finalization_timestamp: u64,
}


/// Represents a request for DAG synchronization.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DAGSyncRequest {
    pub round_id: u64,
    pub sender_id: usize,
    pub sender_url: String,
}

