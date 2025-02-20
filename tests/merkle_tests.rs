use aleph_research::utils::merkle_utils::{
    compute_merkle_root, compute_merkle_branch, validate_merkle_branch, reconstruct_unit, split_into_shards,
};

use sha2::{Digest, Sha256};
use tracing::{info, error};
use tracing_subscriber;

mod tests {
    use super::*;
    use aleph_research::{handlers::handle_prevote::handle_prevote, structs::{node::Node, requests::PrevoteRequest}, utils::create_transaction_data::create_transaction_data};
    use base64::Engine;
    use reqwest::Client;
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


    
    
    

    // #[test]
    // fn test_validate_merkle_branch_complex() {
    //     init_logger();
    
    //     // ✅ Generate mock data shards for testing
    //     let data: Vec<Vec<u8>> = (0..8) // 8 shards for increased complexity
    //         .map(|i| {
    //             let mut shard = vec![0; 256]; // Each shard is 256 bytes
    //             shard[0..6].copy_from_slice(format!("shard{}", i + 1).as_bytes()); // Add identifier
    //             shard
    //         })
    //         .collect();
    
    //     // ✅ Compute SHA-256 hashes for each shard
    //     let shard_hashes: Vec<Vec<u8>> = data.iter().map(|shard| Sha256::digest(shard).to_vec()).collect();
    
    //     // ✅ Compute the Merkle root
    //     let merkle_root = compute_merkle_root(&shard_hashes);
    
    //     // ✅ Compute Merkle proofs for each shard
    //     let proofs: Vec<Vec<u8>> = (0..shard_hashes.len())
    //         .map(|i| compute_merkle_branch(&shard_hashes, i))
    //         .collect::<Vec<Vec<u8>>>();
    
    //     // ✅ Simulate handle_propose, handle_prevote, and handle_commit phases
    //     let mut all_valid = true;
    //     for (i, proof) in proofs.iter().enumerate() {
    //         info!(
    //             "🔹 Testing validation for shard index {}: Proof = {:?}, Expected root = {:?}",
    //             i, proof, merkle_root
    //         );
    
    //         // ✅ Validate the Merkle branch using correct proof format
    //         let is_valid = validate_merkle_branch(&shard_hashes, &[*proof.clone()], i, &merkle_root);
    
    //         if !is_valid {
    //             error!(
    //                 "❌ Validation failed for shard index {}. Proof = {:?}, Expected root = {:?}",
    //                 i, proof, merkle_root
    //             );
    //             all_valid = false;
    //         }
    //     }
    
    //     // ✅ Ensure all Merkle branches are valid
    //     assert!(all_valid, "❌ Some Merkle branch validations failed.");
        
    //     // ✅ Mock reconstruction of a unit and commit phase validation
    //     let reconstructed_unit: Vec<u8> = data.concat(); // Concatenate all shards
    //     let reconstructed_shard_hashes: Vec<Vec<u8>> = reconstructed_unit
    //         .chunks(256) // Simulate splitting back into shards
    //         .map(|shard| Sha256::digest(shard).to_vec())
    //         .collect();
    
    //     let commit_proofs: Vec<Vec<u8>> = (0..reconstructed_shard_hashes.len())
    //         .map(|i| compute_merkle_branch(&reconstructed_shard_hashes, i))
    //         .collect::<Vec<Vec<u8>>>();
    
    //     for (i, proof) in commit_proofs.iter().enumerate() {
    //         let commit_valid = validate_merkle_branch(&reconstructed_shard_hashes, &[*proof.clone()], i, &merkle_root);
    
    //         assert!(
    //             commit_valid,
    //             "❌ Commit phase validation failed for shard index {}.",
    //             i
    //         );
    //     }
    
    //     info!("✅ All complex Merkle branch validations passed.");
    // }
    
    
    


    #[tokio::test]
    async fn test_full_transaction_flow() {
        use std::sync::Arc;
        use base64::engine::general_purpose;
        use sha2::{Digest, Sha256};
        use tokio::sync::Mutex;
        use tracing::info;
        use reqwest::Client;
        use anyhow::Error;

    
        let client = Arc::new(Client::new());
    
        // ✅ **Create the Node Once and Wrap it in `Arc<Mutex<Node>>`**
        let node = Arc::new(Mutex::new(Node::new(
            1, // Node ID
            3, // Total nodes in network
            "10.0.0.1".to_string(), // IP Address
            vec!["10.0.0.2:30333".to_string(), "10.0.0.3:30333".to_string()], // Peers
            "10.0.0.100".to_string(), // IP Manager
            2, // Number of Transactions
            256, // Transaction Size
            4, // Data Shards
            1, // Total Rounds
            client.clone(), // Pass the client
        )));
    
        info!("✅ Node created successfully.");
    
        // ✅ **Create 3 proposals without wrapping the node again**
        let mut proposals = Vec::new();
        for _ in 0..3 {
            let proposal = create_transaction_data(node.clone()).await.unwrap();
            proposals.push(proposal);
        }
    
        info!("✅ Created 3 transaction proposals");
    
        // ✅ **Construct a PREVOTE request with all proposals**
        let prevote_request = PrevoteRequest {
            proposals: proposals.clone(),
            sender_url: "test-node".to_string(),
        };
    
        // ✅ **Mimic the handle_prevote logic**
        let result = handle_prevote(node.clone(), client.clone(), prevote_request).await;
    
        // ✅ **Assert that prevote validation passes**
        assert!(
            result.is_ok(),
            "❌ Prevote validation failed! Expected success but got error: {:?}",
            result
        );
    
        info!("✅ Prevote validation passed!");
    
        // ✅ **Validate each transaction individually**
        for proposal in proposals {
            for transaction in &proposal.transactions {
                // Decode shards
                let decoded_shards: Vec<Vec<u8>> = transaction.shards
                    .iter()
                    .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()).unwrap())
                    .collect();
    
                // Compute hash of each decoded shard
                let shard_hashes: Vec<Vec<u8>> = decoded_shards
                    .iter()
                    .map(|shard| Sha256::digest(shard).to_vec())
                    .collect();
    
                // Compute Merkle root from hashed shards
                let computed_merkle_root = compute_merkle_root(&shard_hashes);
                assert_eq!(
                    computed_merkle_root, transaction.root,
                    "❌ Merkle root mismatch! Expected {:?}, but computed {:?}",
                    transaction.root, computed_merkle_root
                );
    
                info!("✅ Merkle root validated successfully!");
    
                // Validate Merkle proofs
                for (shard_index, _) in decoded_shards.iter().enumerate() {
                    let decoded_proof: Vec<Vec<u8>> = transaction.proofs[shard_index]
                        .iter()
                        .map(|p| general_purpose::STANDARD.decode(p.as_bytes()).unwrap())
                        .collect();
    
                    assert!(
                        validate_merkle_branch(&shard_hashes[shard_index], &decoded_proof, shard_index, &transaction.root),
                        "❌ Merkle proof validation failed for shard index {}",
                        shard_index
                    );
                }
                info!("✅ Merkle proof validated successfully!");
            }
        }
    
        info!("✅ All transactions successfully validated!");
    }
    
       

       
    
   
    
}

