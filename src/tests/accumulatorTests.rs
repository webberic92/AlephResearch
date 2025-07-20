#[cfg(test)]
mod accumulator_tests {
    use crate::{
        structs::{
            node::Node,
            requests::{PrevoteRequest, ProposeRequest},
        },
        utils::{
            create_transaction_data::create_transaction_data,
            rsa_accumulator_util::{memoized_hash_to_prime, compute_accumulator_from_primes},
        },
        handlers::{handle_propose, handle_prevote},
    };
    use base64::{engine::general_purpose, Engine as _};
    use num_bigint::{BigInt, Sign};
    use sha2::{Digest, Sha256};

    fn compute_proposal_digest(proposals: &[ProposeRequest]) -> String {
        let mut proposal_hashes: Vec<String> = proposals
            .iter()
            .map(|p| {
                let id_bytes = format!("{}-{}", p.base.proposing_node_id, p.base.round_id).into_bytes();
                hex::encode(Sha256::digest(&id_bytes))
            })
            .collect();
        proposal_hashes.sort();
        let combined_input: Vec<u8> = proposal_hashes.concat().into_bytes();
        hex::encode(Sha256::digest(&combined_input))
    }
    #[tokio::test]
    async fn test_valid_batch_accumulator_verification() {
        let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let proposal = create_transaction_data(node).await.unwrap();
    
        // Collect ALL shard hashes from ALL transactions
        let mut all_shard_hashes: Vec<String> = proposal.transactions
            .iter()
            .flat_map(|tx| tx.shard_hashes.clone())
            .collect();
    
        all_shard_hashes.sort();
        all_shard_hashes.dedup();
    
        let acc_bytes = general_purpose::STANDARD.decode(&proposal.batch_accumulator).unwrap();
        let stored_accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
    
        let futures: Vec<_> = all_shard_hashes
            .iter()
            .map(|hex| crate::utils::rsa_accumulator_util::memoized_hash_to_prime(hex))
            .collect();
    
        let primes: Vec<BigInt> = futures::future::join_all(futures).await;
        let recomputed_accumulator = compute_accumulator_from_primes(&primes);
    
        assert_eq!(stored_accumulator, recomputed_accumulator, "❌ Batch accumulator mismatch");
    }
    
    
    
    

    #[tokio::test]
    async fn test_invalid_proof_fails() {
        let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let proposal = create_transaction_data(node).await.unwrap();
        let tx = &proposal.transactions[0];

        let acc_bytes = general_purpose::STANDARD.decode(&tx.accumulator).unwrap();
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

        let fake_proof = BigInt::from(999u64);
        let fake_hash = vec![1u8; 32];

        let result = crate::utils::rsa_accumulator_util::verify_proof(&accumulator, &fake_hash, &fake_proof).await;
        assert!(!result, "❌ Fake proof verification unexpectedly passed");
    }

    #[tokio::test]
    async fn test_propose_to_prevote_batch_accumulator_validation_with_digest() {
        let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1, "".into());

        let proposal = create_transaction_data(proposer.clone()).await.unwrap();
        let result = handle_propose::handle_propose(verifier.clone(), proposal.clone()).await;
        assert!(result.is_ok(), "❌ handle_propose rejected a valid proposal");

        let proposals = vec![proposal.clone()];
        let proposal_digest = compute_proposal_digest(&proposals);

        for sender_id in 0..5 {
            let prevote = PrevoteRequest {
                proposals: proposals.clone(),
                batch_accumulator: proposal.batch_accumulator.clone(),
                sender_id,
                sender_url: format!("127.0.0.1:808{}", sender_id),
                proposal_digest: proposal_digest.clone(),
            };
            let _ = handle_prevote::handle_prevote(verifier.clone(), prevote).await;
        }
    }

    #[tokio::test]
    async fn test_batch_accumulator_invalid_should_fail() {
        let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1, "".into());

        let proposal = create_transaction_data(proposer.clone()).await.unwrap();
        let proposals = vec![proposal.clone()];
        let proposal_digest = compute_proposal_digest(&proposals);

        let mut acc_bytes = general_purpose::STANDARD.decode(&proposal.batch_accumulator).unwrap();
        acc_bytes[0] ^= 0xFF;  // Tamper with accumulator
        let tampered = general_purpose::STANDARD.encode(&acc_bytes);

        for sender_id in 0..5 {
            let prevote = PrevoteRequest {
                proposals: proposals.clone(),
                batch_accumulator: tampered.clone(),
                sender_id,
                sender_url: format!("127.0.0.1:808{}", sender_id),
                proposal_digest: proposal_digest.clone(),
            };
            let result = handle_prevote::handle_prevote(verifier.clone(), prevote).await;
            if result.is_err() {
                return;
            }
        }
        panic!("❌ Tampered batch_accumulator should fail but didn't");
    }

    #[tokio::test]
    async fn test_proposal_digest_mismatch_should_fail() {
        let proposer = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let verifier = Node::new(1, 5, "127.0.0.1:8081".into(), vec![], "".into(), 2, 256, 4, 1, "".into());

        let proposal = create_transaction_data(proposer.clone()).await.unwrap();
        let proposals = vec![proposal.clone()];
        let mut proposal_digest = compute_proposal_digest(&proposals);
        proposal_digest.replace_range(0..2, "ff");  // Intentionally corrupt digest

        let prevote = PrevoteRequest {
            proposals: proposals.clone(),
            batch_accumulator: proposal.batch_accumulator.clone(),
            sender_id: 0,
            sender_url: "127.0.0.1:8080".into(),
            proposal_digest,
        };

        let result = handle_prevote::handle_prevote(verifier.clone(), prevote).await;
        assert!(result.is_err(), "❌ Mismatched proposal_digest should fail");
    }

    #[tokio::test]
    async fn test_memoized_hash_to_prime_integration() {
        let node = Node::new(0, 5, "127.0.0.1:8080".into(), vec![], "".into(), 2, 256, 4, 1, "".into());
        let proposal = create_transaction_data(node).await.unwrap();
        let tx = &proposal.transactions[0];
        let shard_hashes = &tx.shard_hashes;

        for (j, hash_bytes) in shard_hashes.iter().enumerate() {
            let prime = memoized_hash_to_prime(&hex::encode(hash_bytes)).await;
            assert!(prime.bits() > 120, "❌ Prime too small for shard[{}]", j);
        }
    }
}
