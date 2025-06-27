use std::collections::HashMap;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use num_bigint::BigInt;
use num_bigint::{ RandBigInt, Sign};
use num_traits::One;
use num_integer::Integer;
use rayon::prelude::*;
use std::sync::Arc;
use dashmap::DashMap;
use rand::SeedableRng;            // Add this
use rand_chacha::ChaCha20Rng;

/// ✅ RSA-1024 modulus
pub fn get_modulus() -> BigInt {
    BigInt::parse_bytes(
        b"134078079299425970995740249982058461274793658205923933\
          77723561443721764030073546976801874298166903427690031",
        10,
    )
    .expect("Failed to parse RSA-1024 modulus")
}


// (proof_hex, prime_hex) -> result
static MODPOW_CACHE: Lazy<DashMap<(String, String), BigInt>> = Lazy::new(DashMap::new);

/// ✅ Cached modular exponentiation
pub fn cached_modpow(proof: &BigInt, prime: &BigInt) -> BigInt {
    let proof_hex = format!("{:x}", proof);
    let prime_hex = format!("{:x}", prime);
    let key = (proof_hex.clone(), prime_hex.clone());

    if let Some(result) = MODPOW_CACHE.get(&key) {
        return result.clone();
    }

    let result = proof.modpow(prime, &get_modulus());
    MODPOW_CACHE.insert(key, result.clone());
    result
}



static GLOBAL_PRIME_CACHE: Lazy<Mutex<HashMap<String, BigInt>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn memoized_hash_to_prime(
   hash_hex: &str,
) -> BigInt {
    {
        let cache = GLOBAL_PRIME_CACHE.lock().await;
        if let Some(existing) = cache.get(hash_hex) {
            return existing.clone();
        }
    }

    // Convert hex back to bytes
    let hash_bytes = hex::decode(hash_hex).expect("Invalid hex in memoized_hash_to_prime");
    let prime = hash_to_prime_128(&hash_bytes).await;

    let mut cache = GLOBAL_PRIME_CACHE.lock().await;
    cache.insert(hash_hex.to_string(), prime.clone());

    prime
}




pub async fn global_hash_to_prime(hash_hex: &str) -> BigInt {
    let mut cache = GLOBAL_PRIME_CACHE.lock().await;
    if let Some(prime) = cache.get(hash_hex) {
        return prime.clone();
    }

    let hash_bytes = match hex::decode(hash_hex) {
        Ok(bytes) => bytes,
        Err(_) => return BigInt::from(0),
    };

    let prime = hash_to_prime_128(&hash_bytes).await;
    cache.insert(hash_hex.to_string(), prime.clone());
    prime
}


/// ✅ Hash to 128-bit prime with internal Miller-Rabin
pub async fn hash_to_prime_128(data: &[u8]) -> BigInt {
    let mut digest = Sha256::digest(data).to_vec();
    for _ in 0..20 {
        let mut candidate_bytes = digest[..16].to_vec();
        candidate_bytes[0] |= 0b1000_0000;
        candidate_bytes[15] |= 0b0000_0001;
        let candidate = BigInt::from_bytes_be(Sign::Plus, &candidate_bytes);
        if is_probably_prime(&candidate, 4) {
            return candidate;
        }
        digest = Sha256::digest(&digest).to_vec();
    }

    BigInt::from_bytes_be(Sign::Plus, &digest[..16])
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

        let mut rng = ChaCha20Rng::from_entropy();        'witness: for _ in 0..k {
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



/// ✅ Tree-based proof generation
pub fn generate_proofs_from_primes_radix(primes: &[BigInt]) -> Vec<BigInt> {
    let n = get_modulus();
    let g = BigInt::from(2u8);
    let len = primes.len();

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

pub fn verify_proofs(acc: &BigInt, pairs: &[(BigInt, BigInt)]) -> bool {
    pairs.par_iter().all(|(prime, proof)| {
        cached_modpow(proof, prime) == *acc
    })
}

pub fn verify_all_batches(accumulators: &[BigInt], proof_batches: &[Vec<(BigInt, BigInt)>]) -> bool {
    accumulators
        .par_iter()
        .zip(proof_batches.par_iter())
        .all(|(acc, pairs)| verify_proofs(acc, pairs))
}


pub fn verify_proof_with_prime(acc: &BigInt, prime: &BigInt, proof: &BigInt) -> bool {
    cached_modpow(proof, prime) == *acc
}


/// ✅ Verifies proof from hash
pub async fn verify_proof(acc: &BigInt, hash: &[u8], proof: &BigInt) -> bool {
    let prime = hash_to_prime_128(hash).await;
    cached_modpow(proof, &prime) == *acc
}

/// ✅ Optional: proof cache scoped per round
pub async fn is_proof_valid_cached(
    prime: &BigInt,
    acc: &BigInt,
    proof: &BigInt,
) -> bool {

    return cached_modpow(proof, prime) == *acc;
}


/// Fully parallel product tree accumulator builder.
pub fn compute_accumulator_from_primes(primes: &[BigInt]) -> BigInt {
    if primes.is_empty() {
        return BigInt::one();
    }

    let modulus = get_modulus();
    
    // Convert to owned Vec for Rayon parallel chunking
    let mut level = primes.to_vec();

    while level.len() > 1 {
        // Pairwise multiply adjacent elements in parallel
        let next_level: Vec<BigInt> = level
            .par_chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    (&chunk[0] * &chunk[1]) % &modulus
                } else {
                    chunk[0].clone()
                }
            })
            .collect();
        level = next_level;
    }

    level[0].clone() % modulus
}


/// Struct for product tree nodes
struct ProductTree {
    product: BigInt,
    left: Option<Arc<ProductTree>>,
    right: Option<Arc<ProductTree>>,
    index_range: (usize, usize),
}

/// Build full balanced product tree
fn build_product_tree(primes: &[BigInt], modulus: &BigInt) -> Arc<ProductTree> {
    fn helper(primes: &[BigInt], range: (usize, usize), modulus: &BigInt) -> Arc<ProductTree> {
        if range.0 == range.1 {
            Arc::new(ProductTree {
                product: primes[range.0].clone() % modulus,
                left: None,
                right: None,
                index_range: range,
            })
        } else {
            let mid = (range.0 + range.1) / 2;
            let (left, right) = rayon::join(
                || helper(primes, (range.0, mid), modulus),
                || helper(primes, (mid + 1, range.1), modulus),
            );
            Arc::new(ProductTree {
                product: (&left.product * &right.product) % modulus,
                left: Some(left),
                right: Some(right),
                index_range: range,
            })
        }
    }
    helper(primes, (0, primes.len() - 1), modulus)
}

/// Generate proof for a single prime using product tree
fn generate_proof_for_index(
    tree: &Arc<ProductTree>,
    index: usize,
    modulus: &BigInt,
) -> BigInt {
    fn helper(
        node: &Arc<ProductTree>,
        index: usize,
        modulus: &BigInt,
    ) -> BigInt {
        if node.index_range.0 == node.index_range.1 {
            return BigInt::from(1);
        }
        let mid = (node.index_range.0 + node.index_range.1) / 2;
        if index <= mid {
            let sibling_product = &node.right.as_ref().unwrap().product;
            (helper(node.left.as_ref().unwrap(), index, modulus) * sibling_product) % modulus
        } else {
            let sibling_product = &node.left.as_ref().unwrap().product;
            (helper(node.right.as_ref().unwrap(), index, modulus) * sibling_product) % modulus
        }
    }
    helper(tree, index, modulus)
}

/// Full public API: generate proofs for entire batch
pub fn generate_proofs_from_primes_tree(primes: &[BigInt]) -> Vec<BigInt> {
    let modulus = crate::utils::rsa_accumulator_util::get_modulus();
    let tree = build_product_tree(primes, &modulus);

    (0..primes.len())
        .into_par_iter()
        .map(|index| generate_proof_for_index(&tree, index, &modulus))
        .collect()
}
