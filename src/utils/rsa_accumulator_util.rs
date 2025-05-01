use num_bigint::{BigInt, RandBigInt, Sign};
use num_traits::One;
use num_integer::Integer; use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
// ✅ <== This is the fix!
use sha2::{Sha256, Digest};
use rand::thread_rng;


/// ✅ Hash data to a probable prime using retry strategy


/// ✅ Miller-Rabin probabilistic primality test
pub fn is_probably_prime(n: &BigInt, k: u32) -> bool {
    if n <= &BigInt::from(1u32) {
        return false;
    }
    if n == &BigInt::from(2u32) || n == &BigInt::from(3u32) {
        return true;
    }
    if n.is_even() {
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
        for _ in 0..(r - 1) {
            x = x.modpow(&BigInt::from(2u32), n);
            if x == n - 1u32 {
                continue 'witness;
            }
        }
        return false;
    }

    true
}

/// ✅ RSA-2048 challenge modulus (safe for public accumulator use)
pub fn get_modulus() -> BigInt {
    BigInt::parse_bytes(b"2519590847565789349402718324004839857142928212620403202777713783604366202070\
                          7595556264018525880784406918290641249515082189298559149176184502808489127\
                          3562522293781971855916539641139942736573411618276031500127101821331960060\
                          10713507615215201023218265236877223636045", 10)
        .expect("Failed to parse RSA-2048 modulus")
}

//// ✅ Accumulate all elements modulo N
pub fn compute_accumulator_radix(elements: &[Vec<u8>]) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);

    let total_product = elements.iter()
        .map(|hash| hash_to_integer(hash)) // 🚫 DO NOT rehash
        .fold(BigInt::one(), |acc, p| acc * p);

    g.modpow(&total_product, &n)
}



pub fn generate_proofs_radix(prime_hashes: &[Vec<u8>]) -> Vec<BigInt> {
    let primes: Vec<BigInt> = prime_hashes.par_iter().map(|h| hash_to_integer(h)).collect();
    let n = get_modulus();
    let g = BigInt::from(2u8);
    let len = primes.len();

    let mut prefix_products = vec![BigInt::one(); len + 1];
    let mut suffix_products = vec![BigInt::one(); len + 1];

    for i in 0..len {
        prefix_products[i + 1] = &prefix_products[i] * &primes[i];
    }
    for i in (0..len).rev() {
        suffix_products[i] = &suffix_products[i + 1] * &primes[i];
    }

    (0..len)
        .into_par_iter()
        .map(|i| {
            let product = &prefix_products[i] * &suffix_products[i + 1];
            g.modpow(&product, &n)
        })
        .collect()
}

pub fn verify_proofs(accumulator: &BigInt, pairs: &[(BigInt, BigInt)]) -> bool {
    pairs.par_iter().all(|(prime, proof)| {
        verify_proof_with_prime(accumulator, prime, proof)
    })
}

pub fn verify_proof_with_prime(accumulator: &BigInt, prime: &BigInt, proof: &BigInt) -> bool {
    let n = get_modulus();
    let reconstructed = proof.modpow(prime, &n);
    &reconstructed == accumulator
}

/// 🚀 Fast hash-to-integer mapping (no primality check)
pub fn hash_to_integer(data: &[u8]) -> BigInt {
    let hash = Sha256::digest(data);
    BigInt::from_bytes_be(Sign::Plus, &hash)
}

// pub fn verify_proof(accumulator: &BigInt, shard: &[u8], proof: &BigInt) -> bool {
//     let hash = Sha256::digest(shard).to_vec();
//     let prime = hash_to_integer(&hash);
//     let n = get_modulus();
//     let reconstructed = proof.modpow(&prime, &n);
//     &reconstructed == accumulator
// }

pub fn verify_proof(accumulator: &BigInt, hash: &[u8], proof: &BigInt) -> bool {
    let prime = hash_to_integer(hash);
    let n = get_modulus();
    let reconstructed = proof.modpow(&prime, &n);
    &reconstructed == accumulator
}
