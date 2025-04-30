#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use base64::{engine::general_purpose, Engine};
    use num_bigint::{BigInt, Sign};
    use reqwest::Client;
    use sha2::{Digest, Sha256};
    use tokio::sync::Mutex;
    use rayon::prelude::*;
    use crate::structs::node::Node;
    use crate::utils::create_transaction_data::{create_transaction_data, pad_to_len};
    use crate::utils::rsa_accumulator_util::{
        compute_accumulator_radix, generate_proofs_radix, get_modulus, hash_to_integer, verify_proof, verify_proofs,
    };
    use reed_solomon_erasure::galois_8::ReedSolomon;
    use num_traits::One;

    #[test]
    fn test_rsa_accumulator_end_to_end() {
        let transaction_size = 256;
        let data_shards = 4;
        let total_shards = 5;

        let tx_content = b"unit-test-tx-001";
        let padded_tx = pad_to_len(tx_content.to_vec(), transaction_size);
        let shard_size = (transaction_size + data_shards - 1) / data_shards;

        let mut data_chunks: Vec<Vec<u8>> = padded_tx
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
        let rs = ReedSolomon::new(data_shards, total_shards - data_shards).unwrap();
        rs.encode(&mut shard_refs).unwrap();

        let primes: Vec<BigInt> = shards.iter().map(|shard| {
            let hash = Sha256::digest(shard).to_vec();
            hash_to_integer(&hash)
        }).collect();

        let g = BigInt::from(2u8);
        let n = get_modulus();
        let total_product = primes.iter().fold(BigInt::one(), |acc, p| acc * p);
        let accumulator = g.modpow(&total_product, &n);

        for (i, shard) in shards.iter().enumerate() {
            let hash = Sha256::digest(shard).to_vec();
            let prime = hash_to_integer(&hash);
            let product_of_others = primes.iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p.clone())
                .fold(BigInt::one(), |acc, p| acc * p);
            let proof = g.modpow(&product_of_others, &n);
            let reconstructed = proof.modpow(&prime, &n);
            assert_eq!(reconstructed, accumulator, "❌ Shard {} proof invalid", i);
        }

        println!("✅ test_rsa_accumulator_end_to_end passed");
    }

    #[test]
    fn test_rsa_accumulator_end_to_end_validation() {
        let data_shards = 4;
        let total_shards = 7;
        let transaction_size = 250;
        let shard_size = (transaction_size + data_shards - 1) / data_shards;
    
        let mut all_hashes = Vec::new();
        for tx_index in 0..6096 {
            let content = format!("tx{}_round{}", tx_index + 1, 1);
            let padded = pad_to_len(content.into_bytes(), transaction_size);
            let rs = ReedSolomon::new(data_shards, total_shards - data_shards).unwrap();
    
            let mut data_chunks: Vec<Vec<u8>> = padded
                .chunks(shard_size)
                .map(|c| {
                    let mut v = c.to_vec();
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
            rs.encode(&mut shard_refs).unwrap();
    
            for shard in shards.iter().take(data_shards) {
                let hash = Sha256::digest(shard).to_vec();
                all_hashes.push(hash);
            }
        }
    
        let acc = compute_accumulator_radix(&all_hashes);
        let primes: Vec<BigInt> = all_hashes.par_iter()
        .map(|hash| hash_to_integer(hash))
        .collect();

        let g = BigInt::from(2u8);
        let n = get_modulus();
        let total_product = primes.iter().fold(BigInt::one(), |acc, p| acc * p);
        let expected_acc = g.modpow(&total_product, &n);
        
        assert_eq!(acc, expected_acc, "❌ Accumulator mismatch");

    
        println!("✅ test_rsa_accumulator_end_to_end_validation passed");
    }
    

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
            for (shard_index, shard) in tx.shards.iter().enumerate() {
                if shard.proofs.is_empty() {
                    continue;
                }

                let shard_bytes = general_purpose::STANDARD
                    .decode(&shard.shard_b64)
                    .expect("Decode shard failed");
                let proof_bytes = general_purpose::STANDARD
                    .decode(&shard.proofs[0])
                    .expect("Decode proof failed");

                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let hash = Sha256::digest(&shard_bytes).to_vec();

                assert!(
                    verify_proof(&accumulator, &hash, &proof),
                    "❌ Verification failed for tx[{}] shard[{}]", tx_index, shard_index
                );
            }
        }

        println!("✅ test_create_and_verify_transaction_batch passed!");
    }

    #[tokio::test]
    async fn test_rsa_accumulator_proof_validation_post_encoding() {
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
            for (shard_index, shard) in tx.shards.iter().enumerate() {
                if shard.proofs.is_empty() {
                    continue;
                }

                let shard_bytes = general_purpose::STANDARD
                    .decode(&shard.shard_b64)
                    .expect("Failed to decode shard");
                let proof_bytes = general_purpose::STANDARD
                    .decode(&shard.proofs[0])
                    .expect("Failed to decode proof");
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
                let hash = Sha256::digest(&shard_bytes).to_vec();

                assert!(
                    verify_proof(&accumulator, &hash, &proof),
                    "❌ Verification failed for tx[{}] shard[{}]", tx_index, shard_index
                );
            }
        }

        println!("✅ test_rsa_accumulator_proof_validation_post_encoding passed!");
    }
}
