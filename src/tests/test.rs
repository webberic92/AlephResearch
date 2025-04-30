#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use reed_solomon_erasure::galois_8::ReedSolomon;
    use reqwest::Client;
    use sha2::{Digest, Sha256};
    use crate::{handlers::handle_prevote::handle_prevote, structs::{node::Node, requests::PrevoteRequest}, utils::{
        create_transaction_data::{create_transaction_data, pad_to_len},
        merkle_utils::{compute_merkle_branch, compute_merkle_root, validate_merkle_branch, verify_merkle_proof},
    }};

    const TX_SIZE: usize = 256; // ✅ Make this configurable if needed

    fn generate_mock_tx_hashes(n: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|i| {
                let content = format!("tx_content_{}", i);
                let padded = pad_to_len(content.into_bytes(), TX_SIZE);
                Sha256::digest(&padded).to_vec()
            })
            .collect()
    }

    #[test]
    fn test_merkle_proof_verification_round_trip() {
        let tx_hashes = generate_mock_tx_hashes(5); // Try different values: 3, 4, 5, 7, 8, 9

        let root = compute_merkle_root(&tx_hashes);
        assert_eq!(root.len(), 32, "Merkle root should be 32 bytes");

        for (i, leaf) in tx_hashes.iter().enumerate() {
            let proof = compute_merkle_branch(&tx_hashes, i);
            let valid = verify_merkle_proof(leaf, &proof, &root, i);

            assert!(
                valid,
                "Merkle proof failed for index {}: leaf = {:x?}, proof = {:?}, root = {:x?}",
                i,
                leaf,
                proof.iter().map(hex::encode).collect::<Vec<_>>(),
                root
            );
        }
    }

    #[test]
    fn test_merkle_proof_should_fail_on_wrong_index() {
        let tx_hashes = generate_mock_tx_hashes(4);
        let root = compute_merkle_root(&tx_hashes);

        let valid_index = 1;
        let wrong_index = 2;
        let leaf = &tx_hashes[valid_index];
        let proof = compute_merkle_branch(&tx_hashes, valid_index);

        let valid = verify_merkle_proof(leaf, &proof, &root, wrong_index);
        assert!(
            !valid,
            "Proof should fail if index is incorrect (used {}, expected {})",
            wrong_index,
            valid_index
        );
    }

    #[test]
    fn test_merkle_proof_should_fail_on_wrong_leaf() {
        let tx_hashes = generate_mock_tx_hashes(4);
        let root = compute_merkle_root(&tx_hashes);

        let valid_index = 1;
        let wrong_leaf = vec![0u8; 32]; // bogus leaf
        let proof = compute_merkle_branch(&tx_hashes, valid_index);

        let valid = verify_merkle_proof(&wrong_leaf, &proof, &root, valid_index);
        assert!(
            !valid,
            "Proof should fail if leaf is incorrect"
        );
    }

    #[test]
    fn test_merkle_tree_end_to_end_validation() {
        use crate::utils::create_transaction_data::pad_to_len;
        use crate::utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, verify_merkle_proof};
        use sha2::{Sha256, Digest};
    
        let data_shards = 4;
        let total_shards = 7;
        let transaction_size = 250;
        let shard_size = (transaction_size + data_shards - 1) / data_shards;
        let total_txs = 8192;
    
        let mut tx_hashes = Vec::with_capacity(total_txs);
    
        for tx_index in 0..total_txs {
            let content = format!("tx{}_round{}", tx_index + 1, 1);
            let padded = pad_to_len(content.into_bytes(), transaction_size);
    
            // Simulate erasure encoding (like RSA test)
            let rs = reed_solomon_erasure::galois_8::ReedSolomon::new(data_shards, total_shards - data_shards)
                .expect("RS init failed");
    
            let mut data_chunks: Vec<Vec<u8>> = padded
                .chunks(shard_size)
                .map(|chunk| {
                    let mut v = chunk.to_vec();
                    v.resize(shard_size, 0);
                    v
                })
                .collect();
    
            while data_chunks.len() < data_shards {
                data_chunks.push(vec![0u8; shard_size]);
            }
    
            let mut shards = data_chunks.clone();
            while shards.len() < total_shards {
                shards.push(vec![0u8; shard_size]);
            }
    
            let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
            rs.encode(&mut shard_refs).expect("RS encoding failed");
    
            // Hash padded tx directly (Merkle tree works on full tx not shards)
            let tx_hash = Sha256::digest(&padded).to_vec();
            tx_hashes.push(tx_hash);
        }
    
        let root = compute_merkle_root(&tx_hashes);
        assert_eq!(root.len(), 32, "Merkle root must be 32 bytes");
    
        for (i, hash) in tx_hashes.iter().enumerate() {
            let proof = compute_merkle_branch(&tx_hashes, i);
            assert!(
                verify_merkle_proof(hash, &proof, &root, i),
                "❌ Merkle proof failed for tx[{}]", i
            );
        }
    
        println!("✅ test_merkle_tree_end_to_end_validation passed");
    }

    #[tokio::test]
    async fn test_handle_prevote_end_to_end_integration_small() {

    
        // Step 1: Initialize test node
        let node_id = 0;
        let node = Node::new(
            node_id,
            5,                                // total_nodes ✅ FIXED
            "127.0.0.1:30333".into(),
            vec![],
            "127.0.0.1:9999".into(),
            3,                                // number_of_transactions
            256,                              // transaction_size
            3,                                // data_shards ✅ FIXED
            1,
            Arc::new(Client::new()),
        );
    
        // Step 2: Create a full transaction proposal from this node
        let propose_request = create_transaction_data(node.clone())
            .await
            .expect("Failed to create transaction data");
    
        // Step 3: Wrap into a PrevoteRequest with same node as sender
        let prevote_request = PrevoteRequest {
            proposals: vec![propose_request.clone()],
            sender_url: "127.0.0.1:30333".into(),
            sender_id: 1,
        };
    
        // Step 4: Simulate the prevote handling
        let client = Arc::new(Client::new());
        let result = handle_prevote(node.clone(), client, prevote_request).await;
    
        // Step 5: Assert it succeeded (i.e., shard was reconstructed + hash and proof verified)
        assert!(
            result.is_ok(),
            "Expected handle_prevote to succeed, but got error: {:?}",
            result.err()
        );
    }
    
    #[tokio::test]
    async fn test_handle_prevote_end_to_end_integration_large() {

    
        // Step 1: Initialize test node
        let node_id = 0;
        let node = Node::new(
            node_id,
            10,                                // total_nodes ✅ FIXED
            "127.0.0.1:30333".into(),
            vec![],
            "127.0.0.1:9999".into(),
            1028,                                // number_of_transactions
            256,                              // transaction_size
            7,                                // data_shards ✅ FIXED
            1,
            Arc::new(Client::new()),
        );
    
        // Step 2: Create a full transaction proposal from this node
        let propose_request = create_transaction_data(node.clone())
            .await
            .expect("Failed to create transaction data");
    
        // Step 3: Wrap into a PrevoteRequest with same node as sender
        let prevote_request = PrevoteRequest {
            proposals: vec![propose_request.clone()],
            sender_url: "127.0.0.1:30333".into(),
            sender_id: 1,
        };
    
        // Step 4: Simulate the prevote handling
        let client = Arc::new(Client::new());
        let result = handle_prevote(node.clone(), client, prevote_request).await;
    
        // Step 5: Assert it succeeded (i.e., shard was reconstructed + hash and proof verified)
        assert!(
            result.is_ok(),
            "Expected handle_prevote to succeed, but got error: {:?}",
            result.err()
        );
    }

        #[test]
        fn test_rs_encoding_and_merkle_verification() {
            let data_shards = 4;
            let total_nodes = 7;
            let transaction_size = 256;
            let shard_size = (transaction_size + data_shards - 1) / data_shards;
        
            let tx_data = b"tx_test_round1".to_vec();
            let mut padded = tx_data.clone();
            padded.resize(transaction_size, 0);
        
            let tx_hash = Sha256::digest(&padded).to_vec();
        
            // Encode with RS
            let rs = ReedSolomon::new(data_shards, total_nodes - data_shards).unwrap();
        
            let mut data_chunks: Vec<Vec<u8>> = padded
                .chunks(shard_size)
                .map(|chunk| {
                    let mut v = chunk.to_vec();
                    v.resize(shard_size, 0);
                    v
                })
                .collect();
        
            while data_chunks.len() < data_shards {
                data_chunks.push(vec![0u8; shard_size]);
            }
        
            let mut shards = data_chunks.clone();
            while shards.len() < total_nodes {
                shards.push(vec![0u8; shard_size]);
            }
        
            let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
            rs.encode(&mut shard_refs).unwrap();
        
            // Simulate reconstructing from first `data_shards` shards
            let mut received: Vec<Option<Vec<u8>>> = shards
                .into_iter()
                .enumerate()
                .map(|(i, shard)| if i < data_shards { Some(shard) } else { None })
                .collect();
        
            // Reconstruct missing shards
            rs.reconstruct(&mut received).unwrap();
        
            // Join reconstructed shards
            let reconstructed: Vec<u8> = received[..data_shards]
                .iter()
                .flat_map(|opt| opt.as_ref().unwrap())
                .cloned()
                .collect();
        
            // Verify that the reconstructed data matches the original padded data
            assert_eq!(reconstructed, padded);
        }
    
}
