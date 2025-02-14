use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructedUnit {
    pub root: Vec<u8>,             // The Merkle root of the unit
    pub parents: Vec<Vec<u8>>,     // Parent unit hashes in binary format
    pub data: Vec<u8>,             // The actual data for the reconstructed unit
    pub round_id: u64,             // The round ID for which this unit belongs
}
