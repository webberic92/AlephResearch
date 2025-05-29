#[cfg(test)]
mod accumulator_tests {
    use std::{sync::Arc, time::Instant};
    use base64::{engine::general_purpose, Engine};
    use num_bigint::{BigInt, Sign};
    use sha2::{Sha256, Digest};
    use crate::{structs::{node::Node, requests::{PrevoteRequest, ProposeRequest}, shard_aggregator::ShardAggregator}, utils::{create_transaction_data::create_transaction_data, rsa_accumulator_util::{compute_accumulator_from_primes, get_modulus, hash_to_prime_128, verify_proof, verify_proof_with_prime}}};

    fn generate_fake_hashes(count: usize) -> Vec<Vec<u8>> {
        (0..count).map(|i| {
            let data = format!("dummy_tx_{}", i).into_bytes();
            Sha256::digest(&data).to_vec()
        }).collect()
    }

    #[tokio::test]
    async fn test_rsa_proposal_valid_proof_snapshot_runtime() {
        let dummy_node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
        let proposal = create_transaction_data(dummy_node).await.unwrap();
    
        let tx = &proposal.transactions[0];
        let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
        let acc_bytes = general_purpose::STANDARD.decode(acc_b64).unwrap();
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
        for (j, hash_hex) in shard_hashes.iter().enumerate() {
            let hash_bytes = hex::decode(hash_hex).unwrap();
            let proof_b64 = &tx.shards[j].proofs[0];
            let proof_bytes = general_purpose::STANDARD.decode(proof_b64).unwrap();
            let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
    
            assert!(
                verify_proof(&accumulator, &hash_bytes, &proof),
                "❌ RSA proof verification failed for shard[{}]", j
            );
        }
    
        println!("✅ RSA proof validation succeeded for all shards.");
    }
    


    #[tokio::test]
    async fn test_invalid_proof_or_shard_order_should_fail() {
        let dummy_node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
        let proposal = create_transaction_data(dummy_node).await.unwrap();
    
        let tx = &proposal.transactions[0];
        let acc_bytes = general_purpose::STANDARD.decode(tx.accumulator.as_ref().unwrap()).unwrap();
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
        for (j, hash_hex) in shard_hashes.iter().enumerate() {
            let hash_bytes = hex::decode(hash_hex).unwrap();
    
            // use unrelated proof
            let fake_data = Sha256::digest(format!("fake_{}", j).as_bytes());
            let unrelated_proof = BigInt::from_bytes_be(Sign::Plus, &fake_data);
    
            assert!(
                !verify_proof(&accumulator, &hash_bytes, &unrelated_proof),
                "❌ Expected invalid proof to fail for shard[{}], but it passed", j
            );
        }
    
        println!("✅ Invalid RSA proofs correctly failed verification.");
    }
    


    #[tokio::test]
 async fn test_propose_to_prevote_cross_validation() {
    let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
    let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1);

    let proposal = create_transaction_data(proposer.clone()).await.unwrap();

    let result = crate::handlers::handle_propose::handle_propose(verifier.clone(), proposal.clone()).await;
    assert!(result.is_ok(), "❌ handle_propose rejected a valid proposal");

    let prevote = PrevoteRequest {
        proposals: vec![proposal],
        sender_url: "127.0.0.1:8080".into(),
        sender_id: 0,
    };

    let result = crate::handlers::handle_prevote::handle_prevote(verifier.clone(), prevote).await;
    assert!(result.is_ok(), "❌ handle_prevote failed to validate reconstructed proposal");


#[tokio::test]
async fn test_invalid_accumulator_detection() {
    let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
    let mut proposal = create_transaction_data(proposer).await.unwrap();

    // Tamper with accumulator
    proposal.batch_accumulator = "ZmFrZV9hY2N1bXVsYXRvcg==".to_string(); // base64("fake_accumulator")

    let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1);
    let result = crate::handlers::handle_propose::handle_propose(verifier, proposal).await;

    assert!(result.is_err(), "❌ Tampered accumulator should have failed verification");
}


#[tokio::test]
async fn test_proofs_generated_by_create_transaction_data_are_valid() {
    let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
    let proposal = create_transaction_data(node).await.unwrap();
    let tx = &proposal.transactions[0];

    let acc_bytes = general_purpose::STANDARD
        .decode(tx.accumulator.as_ref().unwrap())
        .unwrap();
    let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
    let shard_hashes = tx.shard_hashes.as_ref().unwrap();

    for (j, (hash_hex, shard)) in shard_hashes.iter().zip(&tx.shards).enumerate() {
        let hash_bytes = hex::decode(hash_hex).unwrap();
        let proof_bytes = general_purpose::STANDARD.decode(&shard.proofs[0]).unwrap();
        let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

        assert!(
            verify_proof(&accumulator, &hash_bytes, &proof),
            "❌ Proof failed for shard[{}]", j
        );
    }

    println!("✅ Proofs from create_transaction_data verified correctly.");
}


    println!("✅ All generated primes and proofs verified successfully.");

    
    
}

#[test]
fn test_shard_aggregator_multiple_rounds_does_not_panic() {
    let mut aggregator = ShardAggregator::new(2, 4);
    let tx_index = 0;
    let shard_data = vec![0u8; 128];

    aggregator.insert_shard(1, tx_index, 0, shard_data.clone());
    aggregator.insert_shard(1, tx_index, 1, shard_data.clone());
    aggregator.insert_shard(2, tx_index, 0, shard_data.clone()); // Should be ignored

    let reconstructed = aggregator.try_reconstruct(1, tx_index, 256);
    assert!(reconstructed.is_some(), "❌ Reconstruction failed when it should succeed");

    println!("✅ ShardAggregator handles multiple rounds safely.");
}






#[tokio::test]
async fn test_end_to_end_rsa_proof_validation_consistency() {
    use crate::{
        structs::{node::Node, requests::ProposeRequest},
        utils::{create_transaction_data, rsa_accumulator_util::{get_modulus, hash_to_prime_128}},
    };
    use base64::engine::general_purpose;
    use num_bigint::{BigInt, Sign};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let node = Node::new(
        0,
        5,
        "127.0.0.1:8080".into(),
        vec![],
        "".into(),
        2,   // data_shards
        256, // tx_size
        4,   // number of txs
        1,   // total_rounds
    );

    let proposal = create_transaction_data(node.clone())
        .await
        .expect("create_transaction_data failed");

    let serialized = serde_json::to_string(&proposal).expect("serialization failed");
    let deserialized: ProposeRequest =
        serde_json::from_str(&serialized).expect("deserialization failed");

    for (tx_index, tx) in deserialized.transactions.iter().enumerate() {
        let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
        let acc_bytes = general_purpose::STANDARD
            .decode(acc_b64)
            .expect("Failed to decode accumulator");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

        let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard_hashes");

        for (j, shard) in tx.shards.iter().enumerate() {
            if j >= shard_hashes.len() || shard.proofs.is_empty() {
                continue; // skip parity shards or missing proofs
            }

            let proof_b64 = &shard.proofs[0];
            let proof_bytes = general_purpose::STANDARD
                .decode(proof_b64)
                .expect("Failed to decode proof");
            let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

            let hash_bytes =
                hex::decode(&shard_hashes[j]).expect("Failed to decode expected hash");
            let prime = hash_to_prime_128(&hash_bytes);

            let result = proof.modpow(&prime, &get_modulus());

            assert_eq!(
                result, accumulator,
                "❌ RSA proof validation failed at tx {} shard {}",
                tx_index, j
            );
        }
    }
}







}
