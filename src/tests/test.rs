#[cfg(test)]
mod tests {
    use crate::utils::rsa_accumulator_util::{get_modulus, hash_to_prime, verify_proof};
    use crate::utils::create_transaction_data::pad_to_len;
    use num_traits::One;
    use sha2::{Sha256, Digest};
    use num_bigint::BigInt;
    use reed_solomon_erasure::galois_8::ReedSolomon;
    #[cfg(test)]


    #[test]
    fn test_rsa_accumulator_end_to_end() {
        // === Parameters ===
        let transaction_size = 256;
        let data_shards = 4;
        let total_shards = 5;

        // === Simulate transaction creation ===
        let tx_content = b"unit-test-tx-001";
        let padded_tx = pad_to_len(tx_content.to_vec(), transaction_size);
        let shard_size = (transaction_size + data_shards - 1) / data_shards;

        let mut data_chunks: Vec<Vec<u8>> = padded_tx
            .chunks(shard_size)
            .map(|chunk| {
                let mut v = chunk.to_vec();
                v.resize(shard_size, 0); // pad each shard
                v
            })
            .collect();

        while data_chunks.len() < data_shards {
            data_chunks.push(vec![0u8; shard_size]);
        }

        let mut shards = data_chunks.clone();
        while shards.len() < total_shards {
            shards.push(vec![0u8; shard_size]); // add parity shards
        }

        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        let rs = ReedSolomon::new(data_shards, total_shards - data_shards).unwrap();
        rs.encode(&mut shard_refs).unwrap();

        // === RSA accumulator logic ===
        let g = BigInt::from(2u8);
        let n = get_modulus();

        // Map each shard to a hash → prime
        let primes: Vec<BigInt> = shards.iter().map(|shard| {
            let hash = Sha256::digest(shard).to_vec();
            hash_to_prime(&hash)
        }).collect();

        // Create the accumulator by computing g^{product of all primes} mod n
        let total_product = primes.iter().fold(BigInt::one(), |acc, p| acc * p);
        let accumulator = g.modpow(&total_product, &n);

        // === Proof generation and verification ===
        for (i, shard) in shards.iter().enumerate() {
            let hash = Sha256::digest(shard).to_vec();
            let prime = hash_to_prime(&hash);

            let product_of_others = primes.iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p.clone())
                .fold(BigInt::one(), |acc, p| acc * p);

            let proof = g.modpow(&product_of_others, &n);
            let reconstructed = proof.modpow(&prime, &n);

            assert_eq!(
                reconstructed, accumulator,
                "Proof verification failed for shard {}.\nHash: {}\nPrime: {}\nProof: {}\nReconstructed: {}\nAccumulator: {}",
                i,
                hex::encode(&hash),
                prime.to_str_radix(10).chars().take(12).collect::<String>(),
                proof.to_str_radix(16).chars().take(12).collect::<String>(),
                reconstructed.to_str_radix(16).chars().take(12).collect::<String>(),
                accumulator.to_str_radix(16).chars().take(12).collect::<String>(),
            );
        }
    }



    #[tokio::test]
    async fn test_create_and_verify_transaction_batch() {
        use crate::utils::create_transaction_data::create_transaction_data;
        use crate::utils::rsa_accumulator_util::{verify_proof, hash_to_prime, get_modulus};
        use crate::structs::node::Node;
        use std::sync::Arc;
        use tokio::sync::Mutex;
        use reqwest::Client;
        use base64::{engine::general_purpose, Engine};
        use num_bigint::{BigInt, Sign};
        use sha2::{Sha256, Digest};
    
        // === Dummy Node for Test ===
        let dummy_node = Node::new(
            0,                         // id
            10,                         // total_nodes
            "127.0.0.1:3030".to_string(), // IP
            vec![],                    // nodes list
            "".to_string(),            // ip manager
            5,                         // number_of_transactions
            256,                       // transaction_size
            4,                         // data_shards
            2,                         // total_rounds
            Arc::new(Client::new()),
        );
    
        // === Generate Transaction Batch ===
        let proposal = create_transaction_data(dummy_node.clone())
            .await
            .expect("Failed to generate transaction data");
    
        let accumulator_bytes = general_purpose::STANDARD
            .decode(&proposal.batch_accumulator)
            .expect("Failed to decode accumulator");
    
        let accumulator = BigInt::from_bytes_be(Sign::Plus, &accumulator_bytes);
        let n = get_modulus();
    
        // === Verify each shard against its proof ===
        for (tx_index, tx) in proposal.transactions.iter().enumerate() {
            for (shard_index, shard_b64) in tx.shards.iter().enumerate() {
                let shard_bytes = general_purpose::STANDARD
                    .decode(shard_b64)
                    .expect("Failed to decode shard");
    
                let proof_b64 = tx.proofs.get(shard_index)
                    .expect("Missing proof for shard");
    
                let proof_bytes = general_purpose::STANDARD
                    .decode(proof_b64)
                    .expect("Failed to decode proof");
    
                let proof = BigInt::from_bytes_be(Sign::Plus, &proof_bytes);
    
                // === Match production logic ===
                let hash = Sha256::digest(&shard_bytes);
                let prime = hash_to_prime(&hash);
    
                let reconstructed = proof.modpow(&prime, &n);
    
                // === Debug Logging ===
                println!("🔎 tx[{}] shard[{}]", tx_index, shard_index);
                println!("    hash:          {}", hex::encode(&hash));
                println!("    prime:         {}", prime.to_str_radix(10).chars().take(20).collect::<String>());
                println!("    proof:         {}", proof.to_str_radix(10).chars().take(20).collect::<String>());
                println!("    accumulator:   {}", accumulator.to_str_radix(10).chars().take(20).collect::<String>());
                println!("    reconstructed: {}", reconstructed.to_str_radix(10).chars().take(20).collect::<String>());
    
                // === Final Verification ===
                let valid = reconstructed == accumulator;
    
                assert!(
                    valid,
                    "❌ Shard[{}] of tx[{}] failed RSA proof verification!",
                    shard_index,
                    tx_index
                );
            }
        }
    
        println!("✅ All RSA proofs verified correctly from create_transaction_data()");
    }
    
    



}
    
    
    




