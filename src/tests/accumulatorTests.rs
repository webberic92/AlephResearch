#[cfg(test)]
mod accumulator_tests {
    use std::time::Instant;
    use base64::{engine::general_purpose, Engine};
    use num_bigint::{BigInt, Sign};
    use sha2::{Sha256, Digest};
    use crate::{structs::{node::Node, requests::PrevoteRequest, shard_aggregator::ShardAggregator}, utils::{create_transaction_data::create_transaction_data, rsa_accumulator_util::{compute_accumulator_radix, generate_proofs_radix, get_modulus, hash_to_prime_128, verify_proof}}};

    fn generate_fake_hashes(count: usize) -> Vec<Vec<u8>> {
        (0..count).map(|i| {
            let data = format!("dummy_tx_{}", i).into_bytes();
            Sha256::digest(&data).to_vec()
        }).collect()
    }

    #[test]
fn test_shard_aggregator_multiple_rounds_does_not_panic() {
    let mut aggregator = ShardAggregator::new(2, 4); // 2 data shards, 4 total

    let tx_index = 0;
    let shard_data = vec![1u8; 128];

    // Insert valid shards for round 1
    aggregator.insert_shard(1, tx_index, 0, shard_data.clone());
    aggregator.insert_shard(1, tx_index, 1, shard_data.clone());

    // Insert unrelated shard from round 2 (should be ignored safely)
    aggregator.insert_shard(2, tx_index, 0, shard_data.clone());

    // This should NOT panic and should succeed with reconstruction
    let result = aggregator.try_reconstruct(1, tx_index, 256);
    assert!(result.is_some(), "Expected reconstruction for round 1");
}


    #[test]
    fn test_generate_proofs_radix_profiled() {
        let count = 256; // adjust for load
        let hashes = generate_fake_hashes(count);

        let t0 = Instant::now();
        let _acc = compute_accumulator_radix(&hashes);
        println!("🔢 Accumulator computed in: {:?}", t0.elapsed());

        let t1 = Instant::now();
        let proofs = generate_proofs_radix(&hashes);
        println!("🧮 Proofs generated in: {:?}", t1.elapsed());

        assert_eq!(proofs.len(), count);
        println!("✅ test_generate_proofs_radix_profiled passed with {} proofs", count);
    }




    #[cfg(test)]
    mod accumulator_tests {
        use std::sync::Arc;
        use base64::{engine::general_purpose, Engine};
        use num_bigint::{BigInt, Sign};
        use sha2::{Digest, Sha256};
        use crate::structs::node::Node;
        use crate::utils::create_transaction_data::create_transaction_data;
        use crate::utils::rsa_accumulator_util::{verify_proof, hash_to_prime_128, get_modulus};
    
        #[tokio::test]
        async fn test_rsa_proposal_valid_proof_snapshot_runtime() {
            let dummy_node = Node::new(
                0,
                5,
                "127.0.0.1:8080".to_string(),
                vec![],
                "".to_string(),
                2,
                256,
                4,
                1,
            );
    
            let proposal = create_transaction_data(dummy_node)
                .await
                .expect("Failed to generate transaction data");
    
            let tx = &proposal.transactions[0];
            let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
            let acc_bytes = general_purpose::STANDARD.decode(acc_b64).expect("Accumulator decode failed");
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");
    
            println!("\n🧪 Validating generated transaction (tx[0])...");
            println!("Accumulator (base64): {}", acc_b64);
            println!("Accumulator (bytes): 0x{}", hex::encode(&acc_bytes));
    
            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let hash_bytes = hex::decode(hash_hex).expect("Invalid hex");
                let proofs = &tx.shards[j].proofs;
                assert!(!proofs.is_empty(), "Missing proof for shard[{}]", j);
                let proof_b64 = &proofs[0];
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64).expect("Proof decode failed");
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let prime = hash_to_prime_128(&hash_bytes);
                let lhs = proof.modpow(&prime, &get_modulus());
    
                println!("Shard[{}]:", j);
                println!("  hash      = {}", hash_hex);
                println!("  prime     = {}", prime);
                println!("  proof     = {}", hex::encode(&proof_bytes));
                println!("  result    = 0x{}", hex::encode(lhs.to_bytes_be().1.clone()));
    
                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof),
                    "❌ Verification failed for shard[{}]",
                    j
                );
            }
    
            println!("✅ test_rsa_proposal_valid_proof_snapshot_runtime passed");
        }
    }
    


    #[tokio::test]
    async fn test_rsa_proposal_valid_proof_snapshot_runtime() {
        let dummy_node = Node::new(
            0,
            5,
            "127.0.0.1:8080".to_string(),
            vec![],
            "".to_string(),
            2,
            256,
            4,
            1,
        );

        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");

        let tx = &proposal.transactions[0];
        let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
        let acc_bytes = general_purpose::STANDARD.decode(acc_b64).expect("Accumulator decode failed");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");

        println!("\n🧪 Validating generated transaction (tx[0])...");
        println!("Accumulator (base64): {}", acc_b64);
        println!("Accumulator (bytes): 0x{}", hex::encode(&acc_bytes));

        for (j, hash_hex) in shard_hashes.iter().enumerate() {
            let hash_bytes = hex::decode(hash_hex).expect("Invalid hex");
            let proofs = &tx.shards[j].proofs;
            assert!(!proofs.is_empty(), "Missing proof for shard[{}]", j);
            let proof_b64 = &proofs[0];
            let proof_bytes = general_purpose::STANDARD.decode(proof_b64).expect("Proof decode failed");
            let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
            let prime = hash_to_prime_128(&hash_bytes);
            let lhs = proof.modpow(&prime, &get_modulus());

            println!("Shard[{}]:", j);
            println!("  hash      = {}", hash_hex);
            println!("  prime     = {}", prime);
            println!("  proof     = {}", hex::encode(&proof_bytes));
            println!("  result    = 0x{}", hex::encode(lhs.to_bytes_be().1.clone()));

            assert!(
                verify_proof(&accumulator, &hash_bytes, &proof),
                "❌ Verification failed for shard[{}]",
                j
            );
        }

        println!("✅ test_rsa_proposal_valid_proof_snapshot_runtime passed");
    }

    #[tokio::test]
    async fn test_invalid_proof_or_shard_order_should_fail() {
        let dummy_node = Node::new(
            0,
            5,
            "127.0.0.1:8080".to_string(),
            vec![],
            "".to_string(),
            2,
            256,
            4,
            1,
        );
    
        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");
    
        let tx = &proposal.transactions[0];
        let acc_b64 = tx.accumulator.as_ref().unwrap();
        let acc_bytes = general_purpose::STANDARD.decode(acc_b64).unwrap();
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
        println!("\n🧪 Testing with mismatched random data as proof...");
    
        for (j, hash_hex) in shard_hashes.iter().enumerate() {
            let hash_bytes = hex::decode(hash_hex).unwrap();
    
            // 👇 Create a **random unrelated proof** by rotating, double-hashing, and decoding
            let unrelated_data = format!("random_noise_shard_{}", j);
            let unrelated_hash = Sha256::digest(unrelated_data.as_bytes());
            let unrelated_proof = BigInt::from_bytes_be(Sign::Plus, &unrelated_hash);
    
            let result = verify_proof(&accumulator, &hash_bytes, &unrelated_proof);
            assert!(
                !result,
                "❌ Expected verification to fail for hash[{}] with unrelated proof, but it passed!",
                j
            );
        }
    
        println!("✅ test_invalid_proof_or_shard_order_should_fail passed");
    }
    
    #[tokio::test]
    async fn test_current_logic_validates_itself() {
        let dummy_node = Node::new(0, 5, "127.0.0.1:8080".to_string(), vec![], "".to_string(), 1, 256, 4, 1);
        let proposal = create_transaction_data(dummy_node).await.unwrap();
    
        let tx = &proposal.transactions[0];
        let acc_b64 = tx.accumulator.as_ref().unwrap();
        let acc_bytes = base64::engine::general_purpose::STANDARD.decode(acc_b64).unwrap();
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
        for (j, hash_hex) in shard_hashes.iter().enumerate() {
            let hash_bytes = hex::decode(hash_hex).unwrap();
            let proof_b64 = &tx.shards[j].proofs[0];
            let proof_bytes = base64::engine::general_purpose::STANDARD.decode(proof_b64).unwrap();
            let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
    
            assert!(
                verify_proof(&accumulator, &hash_bytes, &proof),
                "Proof verification failed for shard {}",
                j
            );
        }
    }
    

    #[tokio::test]
async fn test_invalid_accumulator_detection() {
    let dummy_node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 1, 256, 4, 1);
    let mut proposal = create_transaction_data(dummy_node.clone()).await.unwrap();

    // Tamper the accumulator in tx[0] — simulate wrong accumulator
    proposal.transactions[0].accumulator = Some("ZmFrZV9iYXNlNjRfYWNjdW11bGF0b3I=".into()); // base64("fake_base64_accumulator")

    let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 1, 256, 4, 1);
    let result = crate::handlers::handle_propose::handle_propose(verifier.clone(), proposal).await;

    assert!(result.is_err(), "Tampered accumulator should fail verification");
}








#[tokio::test]
async fn test_propose_to_prevote_cross_validation() {
    let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
    let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1);

    // Step 1: Generate proposal from proposer
    let proposal = create_transaction_data(proposer.clone())
        .await
        .expect("Proposal creation failed");

    // Step 2: Pass it to verifier node via handle_propose
    let result = crate::handlers::handle_propose::handle_propose(verifier.clone(), proposal.clone()).await;
    assert!(result.is_ok(), "handle_propose failed on valid proposal");

    // Step 3: Simulate the prevote step using handle_prevote
    let prevote = PrevoteRequest {
        proposals: vec![proposal],
        sender_url: "127.0.0.1:8080".into(),
        sender_id: 0,
    };

    let result = crate::handlers::handle_prevote::handle_prevote(verifier.clone(), prevote).await;
    assert!(result.is_ok(), "handle_prevote failed on valid prevote");
}




    
}