use std::collections::{HashMap, HashSet};
use tracing::{error, info, warn};
use reed_solomon_erasure::galois_8::ReedSolomon;

/// ShardAggregator holds partial shards for a transaction across prevotes.
pub struct ShardAggregator {
    pub data_shards: usize,
    pub total_shards: usize,
    pub shard_store: HashMap<(u64, usize, usize), Vec<u8>>,
    pub seen_shards: HashSet<(u64, usize, usize)>, // NEW: (round_id, tx_index, sender)
}


impl Default for ShardAggregator {
    fn default() -> Self {
        Self::new(1, 1)
    }
}

impl ShardAggregator {
    pub fn new(data_shards: usize, total_shards: usize) -> Self {
        ShardAggregator {
            data_shards,
            total_shards,
            shard_store: HashMap::new(),
            seen_shards: HashSet::new(),
        }
    }

    pub fn insert_shard(&mut self, round_id: u64, tx_index: usize, sender_id: usize, shard: Vec<u8>) {
        let key = (round_id, tx_index, sender_id);
        if self.seen_shards.contains(&key) {
            return;
        }
        self.shard_store.insert(key, shard);
        self.seen_shards.insert(key);
    }

    /// Try to reconstruct a transaction from collected shards.
    pub fn try_reconstruct(&self, round_id: u64, tx_index: usize, transaction_size: usize) -> Option<Vec<u8>> {
        let mut shards: Vec<Option<Vec<u8>>> = vec![None; self.total_shards];

        for ((r, tx_i, sender), shard) in &self.shard_store {
            if *r == round_id && *tx_i == tx_index && *sender < self.total_shards {
                shards[*sender] = Some(shard.clone());
            }
        }

        let received_count = shards.iter().filter(|s| s.is_some()).count();
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
                "Aggregator: Failed to reconstruct tx[{}] in round {}: {:?}",
                tx_index, round_id, e
            );
            return None;
        }

        let combined_data: Vec<u8> = shards_clone[..self.data_shards]
            .iter()
            .flat_map(|opt| opt.clone().unwrap_or_default())
            .collect();

        if combined_data.len() < transaction_size {
            error!(
                "Aggregator: Reconstructed tx[{}] too short: got {}, expected {}",
                tx_index, combined_data.len(), transaction_size
            );
            return None;
        }

        let padded = combined_data[..transaction_size].to_vec();
        Some(padded)
    }

    pub fn clear_round(&mut self, round_id: u64) {
        self.shard_store.retain(|(r, _, _), _| *r != round_id);
        self.seen_shards.retain(|(r, _, _)| *r != round_id);
    }
}
