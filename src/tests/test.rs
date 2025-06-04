#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use base64::{engine::general_purpose, Engine};
    use num_bigint::{BigInt, Sign};
    use sha2::{Digest, Sha256};
    use crate::structs::node::Node;
    use crate::utils::create_transaction_data::create_transaction_data;
    use crate::utils::rsa_accumulator_util::verify_proof;

    #[tokio::test]
    async fn test_create_and_verify_transaction_batch() {
        let dummy_node = Node::new(
            0, 5, "127.0.0.1:3030".to_string(), vec![], "".to_string(),
            128, 256, 4, 1
        );

        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");

        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
            let acc_bytes = general_purpose::STANDARD.decode(acc_b64).expect("Accumulator decode failed");
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");

            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let shard = &tx.shards[j];
                let proof_b64 = &shard.proofs[0];
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64).expect("Proof decode failed");
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let hash_bytes = hex::decode(hash_hex).expect("Hash decode failed");

                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof).await,
                    "❌ Verification failed for tx[{}] shard[{}]", tx_index, j
                );
            }
        }

        println!("✅ test_create_and_verify_transaction_batch passed");
    }

    #[tokio::test]
    async fn test_accumulator_verifies_only_data_shards() {
        let dummy_node = Node::new(
            0, 10, "127.0.0.1:8080".to_string(), vec![], "".to_string(),
            1, 256, 7, 1
        );

        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");

        let tx = &proposal.transactions[0];
        let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
        let acc_bytes = general_purpose::STANDARD.decode(acc_b64).expect("Accumulator decode failed");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");

        for (j, shard) in tx.shards.iter().enumerate() {
            if j < shard_hashes.len() {
                assert_eq!(shard.proofs.len(), 1, "Shard {} should have a proof", j);
                let proof_bytes = general_purpose::STANDARD.decode(&shard.proofs[0]).expect("Proof decode failed");
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let hash_bytes = hex::decode(&shard_hashes[j]).expect("Hash decode failed");

                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof).await,
                    "❌ Invalid proof for data shard[{}]", j
                );
            } else {
                assert!(shard.proofs.is_empty(), "Parity shard {} should not have a proof", j);
            }
        }

        println!("✅ test_accumulator_verifies_only_data_shards passed");
    }

    #[tokio::test]
    async fn test_rsa_accumulator_proof_validation_post_encoding() {
        let node = Node::new(
            0, 5, "127.0.0.1:8080".to_string(), vec![], "".to_string(),
            2, 256, 4, 1
        );

        let proposal = create_transaction_data(node)
            .await
            .expect("Failed to generate transaction data");

        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let acc_b64 = tx.accumulator.as_ref().expect("Missing accumulator");
            let acc_bytes = general_purpose::STANDARD.decode(acc_b64).expect("Accumulator decode failed");
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");

            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let hash_bytes = hex::decode(hash_hex).unwrap();
                let proof_b64 = &tx.shards[j].proofs[0];
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64).unwrap();
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);

                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof).await,
                    "❌ Invalid proof post-encoding for tx[{}] shard[{}]", tx_index, j
                );
            }
        }

        println!("✅ test_rsa_accumulator_proof_validation_post_encoding passed");
    }

    #[tokio::test]
    async fn test_handle_propose_round_trip() {
        let proposer = Node::new(
            1, 5, "127.0.0.1:8080".into(), vec![], "".into(),
            2, 256, 4, 1
        );
    
        let verifier = Node::new(
            2, 5, "127.0.0.1:8081".into(), vec![], "".into(),
            2, 256, 4, 1
        );
    
        let propose_req = create_transaction_data(proposer.clone())
            .await
            .expect("Failed to create proposal");
    
        let result = crate::handlers::handle_propose::handle_propose(verifier.clone(), propose_req.clone()).await;
        assert!(result.is_ok(), "handle_propose should succeed on valid proposal");
    }

}
