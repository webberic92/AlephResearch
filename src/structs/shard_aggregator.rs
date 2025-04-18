use std::collections::HashMap;
use sha2::{Digest, Sha256};
use tracing::{info, warn, error};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// ShardAggregator holds partial shards for a transaction across prevotes.
pub struct ShardAggregator {
    pub data_shards: usize,
    pub total_shards: usize,
    // Mapping: (round_id, tx_index, sender_id) => shard
    pub shard_store: HashMap<(u64, usize, usize), Vec<u8>>,
}

impl ShardAggregator {
    pub fn new(data_shards: usize, total_shards: usize) -> Self {
        ShardAggregator {
            data_shards,
            total_shards,
            shard_store: HashMap::new(),
        }
    }

    pub fn insert_shard(&mut self, round_id: u64, tx_index: usize, sender_id: usize, shard: Vec<u8>) {
        self.shard_store.insert((round_id, tx_index, sender_id), shard);
    }

    /// Try to reconstruct a transaction from collected shards.
    pub fn try_reconstruct(&self, round_id: u64, tx_index: usize, transaction_size: usize) -> Option<Vec<u8>> {
        let mut shards: Vec<Option<Vec<u8>>> = vec![None; self.total_shards];
    
        for ((r, tx_i, sender), shard) in &self.shard_store {
            if *r == round_id && *tx_i == tx_index && *sender < self.total_shards {
                info!(
                    "Aggregator: Found shard for tx[{}] round {}, sender {} ({} bytes)",
                    tx_index, round_id, sender, shard.len()
                );
                shards[*sender] = Some(shard.clone());
            }
        }
    
        let received_count = shards.iter().filter(|s| s.is_some()).count();
        let received_indices: Vec<_> = shards
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_some())
            .map(|(i, _)| i)
            .collect();
    
        info!(
            "Aggregator: tx[{}] round {} has {} out of {} shards: {:?}",
            tx_index, round_id, received_count, self.total_shards, received_indices
        );
    
        if received_count < self.data_shards {
            warn!(
                "Aggregator: Not enough shards to reconstruct tx[{}] in round {}: have {}, need {}",
                tx_index, round_id, received_count, self.data_shards
            );
            return None;
        }
    
        let mut shards_clone = shards.clone();
        let rs = match ReedSolomon::new(self.data_shards, self.total_shards - self.data_shards) {
            Ok(r) => r,
            Err(e) => {
                error!("ReedSolomon init failed: {:?}", e);
                return None;
            }
        };
    
        if let Err(e) = rs.reconstruct(&mut shards_clone) {
            error!(
                "Aggregator: Failed to reconstruct shards for tx[{}] in round {}: {:?}",
                tx_index, round_id, e
            );
            return None;
        }
    
        // Collect only data shards
        let combined_data: Vec<u8> = shards_clone[..self.data_shards]
            .iter()
            .flat_map(|opt| opt.clone().unwrap_or_default())
            .collect();
        if combined_data.len() < transaction_size {
            error!(
                "Aggregator: Reconstructed data too short: got {}, expected {}",
                combined_data.len(),
                transaction_size
            );
            return None;
        }
        let padded = combined_data[..transaction_size].to_vec();
        // let padded = combined_data[..transaction_size.min(combined_data.len())].to_vec();
        // 🚨 Debug hash from aggregator side
        use sha2::{Sha256, Digest};
        let hash = Sha256::digest(&padded);
        info!(
            "Aggregator: Reconstructed tx[{}] for round {} → {} bytes, hash = {}",
            tx_index,
            round_id,
            padded.len(),
            hex::encode(&hash)
        );
    
        info!(
            "Aggregator: Padded reconstructed tx[{}] bytes = {:?}",
            tx_index,
            padded
        );
    
        Some(padded)
    }
    

    pub fn clear_round(&mut self, round_id: u64) {
        self.shard_store.retain(|(r, _, _), _| *r != round_id);
    }
}
