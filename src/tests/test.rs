#[cfg(test)]
mod tests {
    use reed_solomon_erasure::galois_8::ReedSolomon;
    use sha2::{Digest, Sha256};
    use tracing::info;
    use crate::{handlers::handle_prevote::handle_prevote, structs::{node::Node, requests::{DagUnit, PrevoteRequest}}, utils::{
        create_transaction_data::{create_transaction_data, pad_to_len},
        merkle_utils::{compute_merkle_branch, compute_merkle_root, verify_merkle_proof},
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
        let total_txs = 6096;
    
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
            "test_instance".into(), // instance_type
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
        let result = handle_prevote(node.clone(), prevote_request).await;
    
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
            "test_instance".into(), // instance_type

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
        let result = handle_prevote(node.clone(), prevote_request).await;
    
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
    
        #[tokio::test]
        async fn test_dagunit_parent_retrieval_across_rounds() {


            let node = Node::new(
                0,
                5,
                "127.0.0.1:3000".to_string(),
                vec![],
                "127.0.0.1:9999".to_string(),
                3,
                256,
                3,
                1,
                "test_instance".into(), // instance_type

            );

            // === Round 1: Insert initial unit ===
            let round_1 = 1;
            let parent_unit = DagUnit {
                unit_id: "U1-1".to_string(),
                proposer_node: 0,
                round: round_1,
                transactions: vec![],
                parent_units: vec![],
                merkle_root: vec![0u8; 32],
                finalization_timestamp: 123456789,
            };

            {
                let node_lock = node.lock().await;
                let mut dag = node_lock.dag.lock().await;
                dag.entry(round_1).or_default().push(parent_unit.clone());
            }

            // === Round 2: Insert child unit with parent reference ===
            let round_2 = 2;
            let child_unit = DagUnit {
                unit_id: "U2-1".to_string(),
                proposer_node: 0,
                round: round_2,
                transactions: vec![],
                parent_units: vec![parent_unit.unit_id.clone()],
                merkle_root: vec![0u8; 32],
                finalization_timestamp: 123456790,
            };

            {
                let node_lock = node.lock().await;
                let mut dag = node_lock.dag.lock().await;
                dag.entry(round_2).or_default().push(child_unit.clone());
            }

            // === Check that parent exists ===
            let parent_exists = {
                let node_lock = node.lock().await;
                let dag = node_lock.dag.lock().await;
                dag.values()
                    .flatten()
                    .any(|unit| unit.unit_id == child_unit.parent_units[0])
            };

            assert!(
                parent_exists,
                "Child unit's parent (U1-1) should exist in DAG"
            );
        }

        #[tokio::test]
        async fn test_proposal_generation_and_dag_parent_hashes() {
            // Initialize dummy node with 3 nodes, 2 transactions, 256-byte tx, 2 data shards, 2 rounds
            let node = Node::new(
                1,
                3,
                "127.0.0.1:30333".to_string(),
                vec!["127.0.0.1:30334".to_string(), "127.0.0.1:30335".to_string()],
                "127.0.0.1:8080".to_string(),
                2,
                256,
                2,
                2,
                "test_instance".into(), // instance_type

            );
        
            {
                let node_guard = node.lock().await;
                let parent_unit = crate::structs::requests::DagUnit {
                    unit_id: "U1-1".to_string(),
                    proposer_node: 1,
                    round: 1,
                    transactions: vec![
                        crate::structs::requests::Transaction {
                            root: vec![1; 32], // dummy hash
                            proofs: vec![],
                            shards: vec![],
                        },
                    ],
                    parent_units: vec![],
                    merkle_root: vec![1; 32],
                    finalization_timestamp: 123456789,
                };
            
                let mut dag = node_guard.dag.lock().await;
                dag.insert(1, vec![parent_unit]);
            
                println!("✅ Inserted into DAG: {:?}", dag);
                *node_guard.current_round.lock().await = 2;
            }
        

            {
                let node_guard = node.lock().await;
                let dag = node_guard.dag.lock().await;
                println!("🔍 DAG before proposal: {:?}", dag);
            }
            // Create proposal for round 2
            let proposal = create_transaction_data(node.clone()).await.expect("Failed to create proposal");
        
            // Validate Merkle root for each transaction
            for (i, tx) in proposal.transactions.iter().enumerate() {
                let proof = &proposal.batch_proofs[i];
                assert_eq!(tx.root.len(), 32);
                assert!(
                    verify_merkle_proof(&tx.root, proof, &proposal.batch_root, i),
                    "Merkle proof invalid for tx[{}]", i
                );
            }
        
            // Check that round 2 proposal includes 1 parent
            assert_eq!(proposal.base.round_id, 2);
            assert_eq!(proposal.parents.len(), 1);
            assert_eq!(proposal.parents[0], "U1-1");
        
            info!("✅ Proposal for round 2 correctly included parent unit U1-1");
        }
        
        




        #[tokio::test]
async fn test_get_all_parents_accumulates_units_across_rounds() {
    use crate::structs::requests::{DagUnit, Transaction};
    use crate::structs::node::Node;


    let node = Node::new(
        0,
        3,
        "127.0.0.1:3000".into(),
        vec![],
        "127.0.0.1:9999".into(),
        1,
        250,
        2,
        1,
        "test_instance".into(), // instance_type

    );

    // Insert 1 unit in round 1
    {
        let node_guard = node.lock().await;
        let mut dag = node_guard.dag.lock().await;

        dag.insert(1, vec![DagUnit {
            unit_id: "U1-0".to_string(),
            proposer_node: 0,
            round: 1,
            transactions: vec![Transaction {
                root: vec![0u8; 32],
                proofs: vec![],
                shards: vec![],
            }],
            parent_units: vec![],
            merkle_root: vec![0u8; 32],
            finalization_timestamp: 111,
        }]);
    }

    // Insert 1 unit in round 2
    {
        let node_guard = node.lock().await;
        let mut dag = node_guard.dag.lock().await;

        dag.insert(2, vec![DagUnit {
            unit_id: "U2-0".to_string(),
            proposer_node: 0,
            round: 2,
            transactions: vec![Transaction {
                root: vec![1u8; 32],
                proofs: vec![],
                shards: vec![],
            }],
            parent_units: vec!["U1-0".to_string()],
            merkle_root: vec![1u8; 32],
            finalization_timestamp: 222,
        }]);
    }

    // Insert 1 unit in round 3
    {
        let node_guard = node.lock().await;
        let mut dag = node_guard.dag.lock().await;

        dag.insert(3, vec![DagUnit {
            unit_id: "U3-0".to_string(),
            proposer_node: 0,
            round: 3,
            transactions: vec![Transaction {
                root: vec![2u8; 32],
                proofs: vec![],
                shards: vec![],
            }],
            parent_units: vec!["U1-0".to_string(), "U2-0".to_string()],
            merkle_root: vec![2u8; 32],
            finalization_timestamp: 333,
        }]);
    }

    // Check parents for round 2
    let parents_round_2 = node.lock().await.get_all_parents(2).await;
    assert_eq!(parents_round_2.len(), 1, "Round 2 should have 1 parent");
    assert_eq!(parents_round_2[0], "U1-0");

    // Check parents for round 3
    let parents_round_3 = node.lock().await.get_all_parents(3).await;
    assert_eq!(parents_round_3.len(), 2, "Round 3 should have 2 parents");
    assert!(parents_round_3.contains(&"U1-0".to_string()));
    assert!(parents_round_3.contains(&"U2-0".to_string()));

    println!("✅ test_get_all_parents_accumulates_units_across_rounds passed");
}


    }
    
    
    


