#[cfg(test)]
mod tests {
    use crate::utils::rsa_accumulator_util::{get_modulus, hash_to_prime, verify_proof, verify_proof_from_hash};
    use crate::utils::create_transaction_data::pad_to_len;
    use num_traits::One;
    use sha2::{Sha256, Digest};
    use num_bigint::BigInt;
    use reed_solomon_erasure::galois_8::ReedSolomon;
    #[test]
    fn test_rsa_accumulator_end_to_end() {
        use crate::utils::rsa_accumulator_util::{verify_proof, hash_to_prime, get_modulus};
        use crate::utils::create_transaction_data::pad_to_len;
        use num_traits::One;
        use sha2::{Sha256, Digest};
        use num_bigint::BigInt;
        use reed_solomon_erasure::galois_8::ReedSolomon;
    
        let transaction_size = 256;
        let data_shards = 4;
        let total_nodes = 5;
    
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
        while shards.len() < total_nodes {
            shards.push(vec![0u8; shard_size]);
        }
    
        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
        let rs = ReedSolomon::new(data_shards, total_nodes - data_shards).unwrap();
        rs.encode(&mut shard_refs).unwrap();
    
        let g = BigInt::from(2u8);
        let n: BigInt = get_modulus();
    
        let primes: Vec<BigInt> = shards.iter().map(|s| {
            let hash = Sha256::digest(s).to_vec();
            hash_to_prime(&hash)
        }).collect();
    
        let total_product = primes.iter().fold(BigInt::one(), |acc, p| acc * p);
        let accumulator = g.modpow(&total_product, &n);
    
        for (i, shard) in shards.iter().enumerate() {
            let hash = Sha256::digest(shard).to_vec();
            let exponent = hash_to_prime(&hash);
        
            let product_of_others = primes.iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p.clone())
                .fold(BigInt::one(), |acc, p| acc * p);
        
            let proof = g.modpow(&product_of_others, &n);
        
            let reconstructed = proof.modpow(&exponent, &n);
        
            if &reconstructed != &accumulator {
                println!(
                    "❌ Shard {}: Proof failed\n  hash: {}\n  prime: {}\n  proof: {}\n  reconstructed: {}\n  accumulator: {}",
                    i,
                    hex::encode(&hash),
                    exponent.to_str_radix(10).chars().take(12).collect::<String>(),
                    proof.to_str_radix(16).chars().take(12).collect::<String>(),
                    reconstructed.to_str_radix(16).chars().take(12).collect::<String>(),
                    accumulator.to_str_radix(16).chars().take(12).collect::<String>(),
                );
            }
        
            assert!(
                &reconstructed == &accumulator,
                "Proof should be valid for shard {}",
                i
            );
        }
    }
    
    
    

}
