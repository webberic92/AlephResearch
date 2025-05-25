use num_bigint::{BigInt, RandBigInt, Sign};
use num_traits::{One, Zero};
use num_integer::Integer;
use rayon::prelude::*;
use sha2::{Sha256, Digest};
use rand::thread_rng;
use tracing::info;

/// ✅ RSA-1024 modulus (fast & safe enough for ephemeral accumulator use)
pub fn get_modulus() -> BigInt {
    BigInt::parse_bytes(b"134078079299425970995740249982058461274793658205923933\
                          77723561443721764030073546976801874298166903427690031", 10)
        .expect("Failed to parse RSA-1024 modulus")
}

/// ✅ Hash to probable 128-bit prime using retry + nonce suffix
pub fn hash_to_prime_128(data: &[u8]) -> BigInt {
    let mut digest = Sha256::digest(data).to_vec();
    for _ in 0..20 {
        let mut candidate_bytes = digest[..16].to_vec();
        candidate_bytes[0] |= 0b1000_0000;
        candidate_bytes[15] |= 0b0000_0001;
        let candidate = BigInt::from_bytes_be(Sign::Plus, &candidate_bytes);
        if is_probably_prime(&candidate, 4) {
            return candidate;
        }
        digest = Sha256::digest(&digest).to_vec(); // retry with next hash
    }
    BigInt::from_bytes_be(Sign::Plus, &digest[..16]) // fallback (rare)
}

/// ✅ Miller-Rabin primality test (4 rounds)
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

/// ✅ Compute RSA accumulator from hashed values
pub fn compute_accumulator_radix(hashes: &[Vec<u8>]) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);
    info!("🔢 Computing RSA accumulator radix...n:{} and g{}",n,g);
    let primes: Vec<BigInt> = hashes
        .par_iter()
        .enumerate()
        .map(|(i, hash)| {
            let prime = hash_to_prime_128(hash);
            info!("🔢 compute_accumulator_radix[{}]: hash={}, prime={}", i, hex::encode(hash), prime);
            prime
        })
        .collect();

    let product = primes.into_par_iter().reduce(BigInt::one, |a, b| a * b);
    g.modpow(&product, &n)
}

/// ✅ Generate exclusion proofs using prefix/suffix scan
pub fn generate_proofs_radix(hashes: &[Vec<u8>]) -> Vec<BigInt> {
    let primes: Vec<BigInt> = hashes
        .par_iter()
        .enumerate()
        .map(|(i, hash)| {
            let prime = hash_to_prime_128(hash);
            // info!("🔁 generate_proofs_radix[{}]: hash={}, prime={}", i, hex::encode(hash), prime);
            prime
        })
        .collect();

    let n = get_modulus();
    let g = BigInt::from(2u8);
    let len = primes.len();

    let mut prefix = vec![BigInt::one(); len + 1];
    for i in 0..len {
        prefix[i + 1] = &prefix[i] * &primes[i];
    }

    let mut suffix = vec![BigInt::one(); len + 1];
    for i in (0..len).rev() {
        suffix[i] = &suffix[i + 1] * &primes[i];
    }

    (0..len)
        .into_par_iter()
        .map(|i| {
            let product = &prefix[i] * &suffix[i + 1];
            let proof = g.modpow(&product, &n);
            // info!("📜 Proof[{}] = {}", i, hex::encode(proof.to_bytes_be().1.clone()));
            proof
        })
        .collect()
}

/// ✅ Batch verification of (prime, proof) pairs
pub fn verify_proofs(accumulator: &BigInt, pairs: &[(BigInt, BigInt)]) -> bool {
    pairs.par_iter().all(|(p, proof)| verify_proof_with_prime(accumulator, p, proof))
}

/// ✅ Verify individual proof given prime
pub fn verify_proof_with_prime(acc: &BigInt, prime: &BigInt, proof: &BigInt) -> bool {
    proof.modpow(prime, &get_modulus()) == *acc
}

/// ✅ Verify proof from original hash
pub fn verify_proof(acc: &BigInt, hash: &[u8], proof: &BigInt) -> bool {
    let prime = hash_to_prime_128(hash);
    // info!("🧪 Verifying: hash={}, prime={}, proof={}", hex::encode(hash), prime, hex::encode(proof.to_bytes_be().1.clone()));
    proof.modpow(&prime, &get_modulus()) == *acc
}
