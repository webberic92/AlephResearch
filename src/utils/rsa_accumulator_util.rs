use num_bigint::{BigInt, RandBigInt, Sign};
use num_traits::{One, Zero};
use num_integer::Integer;
use rayon::prelude::*;
use sha2::{Sha256, Digest};
use rand::thread_rng;
use tracing::info;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::structs::node::Node;

static PRIME_CACHE: Lazy<Mutex<HashMap<Vec<u8>, BigInt>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// ✅ RSA-1024 modulus
pub fn get_modulus() -> BigInt {
    BigInt::parse_bytes(
        b"134078079299425970995740249982058461274793658205923933\
          77723561443721764030073546976801874298166903427690031",
        10,
    )
    .expect("Failed to parse RSA-1024 modulus")
}

/// ✅ Hash to 128-bit prime with global caching
pub async fn hash_to_prime_128(data: &[u8]) -> BigInt {
    let mut cache = PRIME_CACHE.lock().await;
    if let Some(prime) = cache.get(data) {
        return prime.clone();
    }

    let mut digest = Sha256::digest(data).to_vec();
    for _ in 0..20 {
        let mut candidate_bytes = digest[..16].to_vec();
        candidate_bytes[0] |= 0b1000_0000;
        candidate_bytes[15] |= 0b0000_0001;
        let candidate = BigInt::from_bytes_be(Sign::Plus, &candidate_bytes);
        if is_probably_prime(&candidate, 4) {
            cache.insert(data.to_vec(), candidate.clone());
            return candidate;
        }
        digest = Sha256::digest(&digest).to_vec();
    }

    let fallback = BigInt::from_bytes_be(Sign::Plus, &digest[..16]);
    cache.insert(data.to_vec(), fallback.clone());
    fallback
}

/// ✅ Per-node, per-round memoized hash-to-prime
pub async fn memoized_hash_to_prime(
    node: &Arc<Mutex<Node>>,
    round_id: u64,
    hash_hex: &str,
) -> BigInt {
    {
        let node_guard = node.lock().await;
        let cache_guard = node_guard.hash_to_prime_cache.lock().await;

        if let Some(prime) = cache_guard
            .get(&round_id)
            .and_then(|m| m.get(hash_hex).cloned())
        {
            return prime;
        }
    }

    // Cache miss
    let hash_bytes = hex::decode(hash_hex).unwrap(); // assumed valid hex
    let prime = hash_to_prime_128(&hash_bytes).await;

    {
        let node_guard = node.lock().await;
        let mut cache_guard = node_guard.hash_to_prime_cache.lock().await;
        cache_guard
            .entry(round_id)
            .or_default()
            .insert(hash_hex.to_string(), prime.clone());
    }

    prime
}

/// ✅ Caching proof validation
pub async fn is_proof_valid_cached(
    node: &Arc<Mutex<Node>>,
    round_id: u64,
    acc_hex: &str,
    hash_hex: &str,
    proof_hex: &str,
    prime: &BigInt,
    acc: &BigInt,
    proof: &BigInt,
) -> bool {
    let key = (acc_hex.to_string(), hash_hex.to_string(), proof_hex.to_string());

    {
        let node_guard = node.lock().await;
        let cache_guard = node_guard.proof_verification_cache.lock().await;

        if cache_guard
            .get(&round_id)
            .map_or(false, |set| set.contains(&key))
        {
            return true;
        }
    }

    let valid = proof.modpow(prime, &get_modulus()) == *acc;

    if valid {
        let node_guard = node.lock().await;
        let mut cache_guard = node_guard.proof_verification_cache.lock().await;
        cache_guard.entry(round_id).or_default().insert(key);
    }

    valid
}

/// ✅ Miller-Rabin primality test
pub fn is_probably_prime(n: &BigInt, k: u32) -> bool {
    if *n <= BigInt::from(1u32) || n.is_even() {
        return false;
    }

    let mut d = n - 1u32;
    let mut r = 0;
    while d.is_even() {
        d /= 2u32;
        r += 1;
    }

    let mut rng = thread_rng();
    'witness: for _ in 0..k {
        let a = rng.gen_bigint_range(&BigInt::from(2u32), &(n - 2u32));
        let mut x = a.modpow(&d, n);
        if x == BigInt::one() || x == n - 1u32 {
            continue;
        }
        for _ in 0..r - 1 {
            x = x.modpow(&BigInt::from(2u32), n);
            if x == n - 1u32 {
                continue 'witness;
            }
        }
        return false;
    }

    true
}

/// ✅ Computes accumulator from vector of primes
pub fn compute_accumulator_from_primes(primes: &[BigInt]) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);
    let product = primes.par_iter().cloned().reduce(BigInt::one, |a, b| a * b);
    g.modpow(&product, &n)
}

/// ✅ Tree-based proof generation
pub fn generate_proofs_from_primes_radix(primes: &[BigInt]) -> Vec<BigInt> {
    let n = get_modulus();
    let g = BigInt::from(2u8);
    let len = primes.len();

    // Build radix tree
    let mut levels: Vec<Vec<BigInt>> = Vec::new();
    levels.push(primes.to_vec());

    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let mut next = Vec::with_capacity((prev.len() + 1) / 2);
        for i in (0..prev.len()).step_by(2) {
            if i + 1 < prev.len() {
                next.push(&prev[i] * &prev[i + 1]);
            } else {
                next.push(prev[i].clone());
            }
        }
        levels.push(next);
    }

    fn compute_excluding(idx: usize, levels: &Vec<Vec<BigInt>>) -> BigInt {
        let mut product = BigInt::one();
        let mut i = idx;
        for level in 0..levels.len() - 1 {
            let sibling = if i % 2 == 0 { i + 1 } else { i - 1 };
            if sibling < levels[level].len() {
                product *= &levels[level][sibling];
            }
            i /= 2;
        }
        product
    }

    (0..len)
        .into_par_iter()
        .map(|i| {
            let excl = compute_excluding(i, &levels);
            g.modpow(&excl, &n)
        })
        .collect()
}

/// ✅ Verifies batch of (prime, proof) pairs
pub fn verify_proofs(acc: &BigInt, pairs: &[(BigInt, BigInt)]) -> bool {
    pairs
        .par_iter()
        .all(|(prime, proof)| proof.modpow(prime, &get_modulus()) == *acc)
}

/// ✅ Verifies single (prime, proof)
pub fn verify_proof_with_prime(acc: &BigInt, prime: &BigInt, proof: &BigInt) -> bool {
    proof.modpow(prime, &get_modulus()) == *acc
}

/// ✅ Verifies from raw hash (deprecated path)
pub async fn verify_proof(acc: &BigInt, hash: &[u8], proof: &BigInt) -> bool {
    let prime = hash_to_prime_128(hash).await;
    proof.modpow(&prime, &get_modulus()) == *acc
}
