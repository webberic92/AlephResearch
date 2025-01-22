use sha2::{Digest, Sha256};
use tracing::{error, info};
use crate::{utils::merkle_utils::{split_into_shards, validate_shard_sizes, compute_merkle_root}, structs::toml_config::TomlConfig};

pub fn create_transaction_data(
    toml_config: &TomlConfig,
) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>), Box<dyn std::error::Error>> {
    
    let transaction_data = vec![1; toml_config.consensus.transaction_size];
    info!(
        "Generated transaction data of size: {} bytes",
        transaction_data.len()
    );

    let shards = split_into_shards(&transaction_data, toml_config.consensus.data_shards);
    info!(
        "Transaction data split into {} shards",
        toml_config.consensus.data_shards
    );

    let proofs: Vec<Vec<u8>> = shards
        .iter()
        .map(|shard| {
            let hash = Sha256::digest(shard).to_vec();
            info!("Computed hash for shard: {:?}", hash);
            hash
        })
        .collect();

    if let Err(e) = validate_shard_sizes(&shards, toml_config.consensus.transaction_size) {
        error!("Shard size validation failed: {}", e);
        return Err(e.into());
    }

    let merkle_root = compute_merkle_root(&proofs);
    info!("Computed Merkle root: {:?}", merkle_root);

    Ok((shards, proofs, merkle_root))
}
