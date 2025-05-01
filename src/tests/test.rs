#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use base64::{engine::general_purpose, Engine};
    use num_bigint::{BigInt, Sign};
    use reqwest::Client;
    use sha2::{Digest, Sha256};
    use crate::structs::node::Node;
    use crate::utils::create_transaction_data::create_transaction_data;
    use crate::utils::rsa_accumulator_util::{verify_proof};
    
    #[tokio::test]
    async fn test_create_and_verify_transaction_batch() {
        let dummy_node = Node::new(
            0, 5, "127.0.0.1:3030".to_string(), vec![], "".to_string(),
            2, 256, 4, 1, Arc::new(Client::new()),
        );

        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");

        let acc_bytes = general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .expect("Failed to decode accumulator");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);

        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");
            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let shard = &tx.shards[j];
                let proof_b64 = &shard.proofs[0];
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64).expect("proof decode");
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let hash_bytes = hex::decode(hash_hex).expect("hash decode");

                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof),
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
            1, 256, 7, 1, Arc::new(Client::new()),
        );

        let proposal = create_transaction_data(dummy_node)
            .await
            .expect("Failed to generate transaction data");
        let tx = &proposal.transactions[0];

        let acc_bytes = general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .expect("decode accumulator");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
        let shard_hashes = tx.shard_hashes.as_ref().expect("missing hashes");

        for (j, shard) in tx.shards.iter().enumerate() {
            if j < shard_hashes.len() {
                assert_eq!(shard.proofs.len(), 1, "Data shard {} should have 1 proof", j);
                let proof = BigInt::from_bytes_be(
                    Sign::Plus,
                    &general_purpose::STANDARD.decode(&shard.proofs[0]).unwrap(),
                );
                let hash_bytes = hex::decode(&shard_hashes[j]).unwrap();
                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof),
                    "❌ Verification failed for data shard {}",
                    j
                );
            } else {
                assert!(
                    shard.proofs.is_empty(),
                    "❌ Parity shard {} should not have any proof",
                    j
                );
            }
        }

        println!("✅ test_accumulator_verifies_only_data_shards passed");
    }



    #[cfg(test)]
    mod tests {
        use std::sync::Arc;
        use base64::{engine::general_purpose, Engine};
        use num_bigint::{BigInt, Sign};
        use reqwest::Client;
        use sha2::{Digest, Sha256};
        use crate::structs::node::Node;
        use crate::utils::create_transaction_data::create_transaction_data;
        use crate::utils::rsa_accumulator_util::{verify_proof};
        
        #[tokio::test]
        async fn test_create_and_verify_transaction_batch() {
            let dummy_node = Node::new(
                0, 5, "127.0.0.1:3030".to_string(), vec![], "".to_string(),
                2, 256, 4, 1, Arc::new(Client::new()),
            );
    
            let proposal = create_transaction_data(dummy_node)
                .await
                .expect("Failed to generate transaction data");
    
            let acc_bytes = general_purpose::STANDARD
                .decode(&proposal.batch_accumulator)
                .expect("Failed to decode accumulator");
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
    
            for (tx_index, tx) in proposal.transactions.iter().enumerate() {
                let shard_hashes = tx.shard_hashes.as_ref().expect("Missing shard hashes");
                for (j, hash_hex) in shard_hashes.iter().enumerate() {
                    let shard = &tx.shards[j];
                    let proof_b64 = &shard.proofs[0];
                    let proof_bytes = general_purpose::STANDARD.decode(proof_b64).expect("proof decode");
                    let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                    let hash_bytes = hex::decode(hash_hex).expect("hash decode");
    
                    assert!(
                        verify_proof(&accumulator, &hash_bytes, &proof),
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
                1, 256, 7, 1, Arc::new(Client::new()),
            );
    
            let proposal = create_transaction_data(dummy_node)
                .await
                .expect("Failed to generate transaction data");
            let tx = &proposal.transactions[0];
    
            let acc_bytes = general_purpose::STANDARD
                .decode(&proposal.batch_accumulator)
                .expect("decode accumulator");
            let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
            let shard_hashes = tx.shard_hashes.as_ref().expect("missing hashes");
    
            for (j, shard) in tx.shards.iter().enumerate() {
                if j < shard_hashes.len() {
                    assert_eq!(shard.proofs.len(), 1, "Data shard {} should have 1 proof", j);
                    let proof = BigInt::from_bytes_be(
                        Sign::Plus,
                        &general_purpose::STANDARD.decode(&shard.proofs[0]).unwrap(),
                    );
                    let hash_bytes = hex::decode(&shard_hashes[j]).unwrap();
                    assert!(
                        verify_proof(&accumulator, &hash_bytes, &proof),
                        "❌ Verification failed for data shard {}",
                        j
                    );
                } else {
                    assert!(
                        shard.proofs.is_empty(),
                        "❌ Parity shard {} should not have any proof",
                        j
                    );
                }
            }
    
            println!("✅ test_accumulator_verifies_only_data_shards passed");
        }
    }

    






    #[tokio::test]
    async fn test_rsa_accumulator_proof_validation_post_encoding() {
        use crate::utils::rsa_accumulator_util::verify_proof;
    
        let node = Node::new(
            0, 5, "127.0.0.1:8080".to_string(), vec![], "".to_string(),
            2, 256, 4, 1, Arc::new(Client::new()),
        );
    
        let proposal = create_transaction_data(node)
            .await
            .expect("Failed to generate proposal");
    
        let acc_bytes = general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .expect("Decode accumulator failed");
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &acc_bytes);
    
        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            let shard_hashes = tx.shard_hashes.as_ref().expect("missing hashes");
            for (j, hash_hex) in shard_hashes.iter().enumerate() {
                let hash_bytes = hex::decode(hash_hex).unwrap();
                let proof_b64 = &tx.shards[j].proofs[0];
                let proof_bytes = general_purpose::STANDARD.decode(proof_b64).unwrap();
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
    
                assert!(
                    verify_proof(&accumulator, &hash_bytes, &proof),
                    "❌ Verification failed for tx[{}] shard[{}]", tx_index, j
                );
            }
        }
    
        println!("✅ test_rsa_accumulator_proof_validation_post_encoding passed");
    }

    









    



















}
