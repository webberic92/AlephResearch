use aleph_research::utils::merkle_utils::{
    compute_merkle_root, compute_merkle_branch, validate_merkle_branch, reconstruct_unit, split_into_shards,
};

use sha2::{Digest, Sha256};
use tracing::{info, error};
use tracing_subscriber;

mod tests {
    use super::*;
    use tracing_subscriber;

    fn init_logger() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .try_init();
    }

    #[test]
    fn test_single_node_merkle_tree() {
        init_logger();

        let hashes = vec![vec![1; 32]];
        let root = compute_merkle_root(&hashes);
        info!("Test single node Merkle tree: Computed root = {:?}", root);
        assert_eq!(root, hashes[0], "Single node Merkle root mismatch");
    }

    #[test]
    fn test_odd_length_merkle_tree() {
        init_logger();

        let data = vec![vec![1; 32], vec![2; 32], vec![3; 32]];
        let root = compute_merkle_root(&data);
        info!("Test odd-length Merkle tree: Computed root = {:?}", root);
        assert!(!root.is_empty(), "Root should not be empty for odd-length tree");
    }

    #[test]
    fn test_validate_merkle_branch_minimal() {
        init_logger();

        let data: Vec<Vec<u8>> = (0..4)
            .map(|i| {
                let mut shard = vec![0; 256];
                shard[0..6].copy_from_slice(format!("shard{}", i + 1).as_bytes());
                shard
            })
            .collect();

        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
        let root = compute_merkle_root(&hashes);

        let proofs: Vec<Vec<Vec<u8>>> = (0..hashes.len())
            .map(|i| compute_merkle_branch(&hashes, i))
            .collect();

        for (i, proof) in proofs.iter().enumerate() {
            info!(
                "Testing validation for shard index {}: Proof = {:?}, Expected root = {:?}",
                i, proof, root
            );
            assert!(
                validate_merkle_branch(&hashes, proof, i, &root),
                "Validation failed for shard index {}. Proof = {:?}, Expected root = {:?}",
                i, proof, root
            );
        }
        info!("All minimal Merkle branch validations passed.");
    }

    #[test]
    fn test_reconstruct_unit() {
        init_logger();

        let transaction_data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let shard_count = 4;

        let shards = split_into_shards(&transaction_data, shard_count);
        let hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
        let root = compute_merkle_root(&hashes);

        let proofs: Vec<Vec<Vec<u8>>> = (0..hashes.len())
            .map(|i| compute_merkle_branch(&hashes, i))
            .collect();

        match reconstruct_unit(&shards, &proofs, &root) {
            Ok(reconstructed_data) => {
                assert_eq!(
                    reconstructed_data, transaction_data,
                    "Reconstructed data does not match the original transaction data"
                );
                info!("Reconstruction and validation succeeded!");
            }
            Err(error_message) => {
                error!("Reconstruction failed: {}", error_message);
                panic!("Reconstruction failed: {}", error_message);
            }
        }
    }

    #[test]
    fn test_validate_merkle_branch_complex() {
        init_logger();

        // Generate mock data shards for testing
        let data: Vec<Vec<u8>> = (0..8) // Increase complexity with more shards
            .map(|i| {
                let mut shard = vec![0; 256]; // Larger shard size
                shard[0..6].copy_from_slice(format!("shard{}", i + 1).as_bytes());
                shard
            })
            .collect();

        // Compute hashes for each shard
        let hashes: Vec<Vec<u8>> = data.iter().map(|d| Sha256::digest(d).to_vec()).collect();
        
        // Compute the Merkle root for the given data
        let root = compute_merkle_root(&hashes);

        // Compute Merkle proofs for each shard
        let proofs: Vec<Vec<Vec<u8>>> = (0..hashes.len())
            .map(|i| compute_merkle_branch(&hashes, i))
            .collect();

        // Simulate handle_propose, handle_prevote, and handle_commit phases
        let mut all_valid = true;
        for (i, proof) in proofs.iter().enumerate() {
            info!(
                "Testing validation for shard index {}: Proof = {:?}, Expected root = {:?}",
                i, proof, root
            );

            // Validate the Merkle branch
            let is_valid = validate_merkle_branch(&hashes, proof, i, &root);

            if !is_valid {
                error!(
                    "Validation failed for shard index {}. Proof = {:?}, Expected root = {:?}",
                    i, proof, root
                );
                all_valid = false;
            }
        }

        // Ensure all Merkle branches are valid
        assert!(all_valid, "Some Merkle branch validations failed.");
        
        // Mock reconstruction of a unit and its subsequent validation in the commit phase
        let reconstructed_unit: Vec<u8> = data.concat(); // Concatenate all shards to form the unit
        let shard_hashes: Vec<Vec<u8>> = reconstructed_unit
            .chunks(256) // Mock shard size
            .map(|shard| Sha256::digest(shard).to_vec())
            .collect();

        let commit_proofs: Vec<Vec<Vec<u8>>> = (0..shard_hashes.len())
            .map(|i| compute_merkle_branch(&shard_hashes, i))
            .collect();

        for (i, proof) in commit_proofs.iter().enumerate() {
            assert!(
                validate_merkle_branch(&shard_hashes, proof, i, &root),
                "Commit phase validation failed for shard index {}.",
                i
            );
        }

        info!("All complex Merkle branch validations passed.");
    }



    #[test]
    fn test_propose_integration() {
        init_logger();
        info!("Starting test_propose_integration...");

        let transaction_data = (0..256).map(|i| i as u8).collect::<Vec<_>>();
        let shard_count = 4;

        let shards = split_into_shards(&transaction_data, shard_count);
        let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|shard| Sha256::digest(shard).to_vec()).collect();
        let root = compute_merkle_root(&shard_hashes);

        let proofs: Vec<Vec<Vec<u8>>> = (0..shard_hashes.len())
            .map(|i| compute_merkle_branch(&shard_hashes, i))
            .collect();

        info!("Generated proofs: {:?}", proofs);

        for (i, proof) in proofs.iter().enumerate() {
            info!(
                "Validating Merkle branch for shard {}: Proof = {:?}, Expected root = {:?}",
                i, proof, root
            );
            assert!(
                validate_merkle_branch(&shard_hashes, proof, i, &root),
                "Validation failed for Shard {}. Proof = {:?}, Expected root = {:?}",
                i, proof, root
            );
        }

        match reconstruct_unit(&shards, &proofs, &root) {
            Ok(reconstructed_data) => {
                assert_eq!(
                    reconstructed_data, transaction_data,
                    "Reconstructed data does not match the original transaction data"
                );
                info!("Reconstruction and validation succeeded!");
            }
            Err(error_message) => {
                error!("Reconstruction failed: {}", error_message);
                panic!("Reconstruction failed: {}", error_message);
            }
        }
    }
}

