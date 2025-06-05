   
    #[cfg(test)]
    mod accumulator_tests {
        use crate::{
            structs::{
                node::Node,
                requests::{PrevoteRequest, ProposeRequest},
                shard_aggregator::ShardAggregator,
            },
            utils::{
                create_transaction_data::create_transaction_data,
                rsa_accumulator_util::{verify_proof, memoized_hash_to_prime},
            },
            handlers::{handle_propose, handle_prevote},
        };
        use base64::{engine::general_purpose, Engine as _};
        use num_bigint::{BigInt, Sign};
        use sha2::{Digest, Sha256};
        use tokio::sync::Mutex;
        use std::sync::Arc;
    
        #[tokio::test]
        async fn test_valid_rsa_proof_verification() {
            let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
            let proposal = create_transaction_data(node).await.unwrap();
            let tx = &proposal.transactions[0];
    
            let acc_bytes = general_purpose::STANDARD.decode(tx.accumulator.as_ref().unwrap()).unwrap();
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let hash_bytes = hex::decode(hash_hex).unwrap();
                let proof_b64 = &tx.shards[j].proofs[0];
                let proof = BigInt::from_bytes_be(Sign::Plus, &general_purpose::STANDARD.decode(proof_b64).unwrap());
    
                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof).await,
                    "❌ RSA proof verification failed for shard[{}]", j
                );
            }
        }
    
        #[tokio::test]
        async fn test_invalid_proof_fails() {
            let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
            let proposal = create_transaction_data(node).await.unwrap();
            let tx = &proposal.transactions[0];
    
            let acc_bytes = general_purpose::STANDARD.decode(tx.accumulator.as_ref().unwrap()).unwrap();
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
            for (j, _) in shard_hashes.iter().enumerate() {
                let fake_hash = Sha256::digest(format!("fake_{}", j)).to_vec();
                let fake_proof = BigInt::from_bytes_be(Sign::Plus, &fake_hash);
    
                assert!(
                    !verify_proof(&accumulator, &fake_hash, &fake_proof).await,
                    "❌ Fake proof should have failed on shard[{}]", j
                );
            }
        }
    
        #[tokio::test]
        async fn test_propose_to_prevote_batch_accumulator_validation() {
            let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
            let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1);
    
            let proposal = create_transaction_data(proposer.clone()).await.unwrap();
            let result = handle_propose::handle_propose(verifier.clone(), proposal.clone()).await;
            assert!(result.is_ok(), "❌ handle_propose rejected a valid proposal");
    
            let prevote = PrevoteRequest {
                proposals: vec![proposal.clone()],
                batch_accumulator: proposal.batch_accumulator.clone(),
                sender_id: 0,
                sender_url: "127.0.0.1:8080".into(),
            };
    
            let result = handle_prevote::handle_prevote(verifier.clone(), prevote).await;
            assert!(result.is_ok(), "❌ handle_prevote failed to validate batch accumulator");
        }
    
        #[tokio::test]
async fn test_batch_accumulator_invalid_should_fail() {
    let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
    let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1);

    let proposal = create_transaction_data(proposer.clone()).await.unwrap();

    let mut acc_bytes = base64::engine::general_purpose::STANDARD
        .decode(&proposal.batch_accumulator)
        .unwrap();
    acc_bytes[0] ^= 0xFF;
    let tampered = base64::engine::general_purpose::STANDARD.encode(&acc_bytes);

    let prevote = PrevoteRequest {
        proposals: vec![proposal.clone()],
        batch_accumulator: tampered,
        sender_id: 99,
        sender_url: "127.0.0.1:8082".into(),
    };

    // Insert enough fake votes to satisfy quorum
    {
        let mut guard = verifier.lock().await;
        let mut votes = guard.quorum_votes.lock().await;
        let key = proposal.base.round_id.to_be_bytes().to_vec();
        votes.entry(key.clone()).or_default().insert("a".into());
        votes.entry(key.clone()).or_default().insert("b".into());
        votes.entry(key).or_default().insert("c".into());
    }

    let result = handle_prevote::handle_prevote(verifier.clone(), prevote).await;

    assert!(
        result.is_err(),
        "❌ Tampered batch_accumulator should fail (got {:?})",
        result
    );
}

        
    
        #[tokio::test]
        async fn test_memoized_hash_to_prime_integration() {
            let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1);
            let proposal = create_transaction_data(node).await.unwrap();
            let tx = &proposal.transactions[0];
            let shard_hashes = tx.shard_hashes.as_ref().unwrap();
    
            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let prime = memoized_hash_to_prime(hash_hex).await;
                assert!(prime.bits() > 120, "❌ Prime too small for shard[{}]", j);
            }
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
            assert!(reconstructed.is_some(), "❌ Reconstruction failed");
        }
    }
    




