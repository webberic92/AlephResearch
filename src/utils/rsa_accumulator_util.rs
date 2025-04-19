use num_bigint::{BigInt, RandBigInt, Sign};
use num_traits::{One, Zero};
use num_integer::Integer; // ✅ <== This is the fix!
use sha2::{Sha256, Digest};
use rand::thread_rng;


/// ✅ Hash data to a probable prime using retry strategy
pub fn hash_to_prime(data: &[u8]) -> BigInt {
    let mut x = Sha256::digest(data).to_vec();
    loop {
        let candidate = BigInt::from_bytes_be(Sign::Plus, &x);
        if is_probably_prime(&candidate, 16) { // 16 rounds of Miller-Rabin
            return candidate;
        }
        x = Sha256::digest(&x).to_vec();
    }
}

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

/// ✅ Accumulate all elements modulo N
pub fn compute_accumulator(elements: &[Vec<u8>]) -> BigInt {
    let n = get_modulus();
    elements.iter()
        .map(|e| hash_to_prime(e))
        .fold(BigInt::one(), |acc, p| acc.modpow(&p, &n))
}

/// ✅ Generate inclusion proof for element[i] (product of others)
pub fn generate_proof(elements: &[Vec<u8>], index: usize, _accumulator: &BigInt) -> BigInt {
    let n = get_modulus();
    elements.iter().enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, e)| hash_to_prime(e))
        .fold(BigInt::one(), |acc, p| acc.modpow(&p, &n))
}

/// ✅ Verify an element against its proof and accumulator
pub fn verify_proof(element: &[u8], proof: &BigInt, accumulator: &BigInt) -> bool {
    let n = get_modulus();
    let p = hash_to_prime(element);
    proof.modpow(&p, &n) == *accumulator
}
