use num_bigint::{BigInt, Sign};
use num_traits::One;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

/// RSA-2048 modulus used for all operations
pub fn get_modulus() -> BigInt {
    BigInt::parse_bytes(b"2519590847565789349402718324004839857142928212620403202777713783604366202070\
                          7595556264018525880784406918290641249515082189298559149176184502808489127\
                          3562522293781971855916539641139942736573411618276031500127101821331960060\
                          10713507615215201023218265236877223636045", 10)
        .expect("Failed to parse RSA-2048 modulus")
}

/// Hash input to a unique prime using retry strategy
pub fn hash_to_prime(data: &[u8]) -> BigInt {
    let mut x = Sha256::digest(data).to_vec();
    loop {
        let candidate = BigInt::from_bytes_be(Sign::Plus, &x);
        if is_probably_prime(&candidate, 16) {
            return candidate;
        }
        x = Sha256::digest(&x).to_vec();
    }
}

/// Miller-Rabin primality test
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

    let mut rng = rand::thread_rng();
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

/// Computes the accumulator using a radix (balanced binary tree) strategy
pub fn compute_accumulator_radix(hashes: &[Vec<u8>]) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);

    // Map to primes in parallel
    let primes: Vec<BigInt> = hashes.par_iter().map(|h| hash_to_prime(h)).collect();

    // Use tree reduction to multiply them
    let total_product = parallel_product(primes);

    g.modpow(&total_product, &n)
}

/// Generates proofs for each hash using radix-style subset exclusion
pub fn generate_proofs_radix(hashes: &[Vec<u8>]) -> Vec<BigInt> {
    let primes: Vec<BigInt> = hashes.par_iter().map(|h| hash_to_prime(h)).collect();
    let n = get_modulus();
    let g = BigInt::from(2u8);
    let len = primes.len();

    // Compute prefix/suffix products
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

/// Parallel recursive product
fn parallel_product(mut items: Vec<BigInt>) -> BigInt {
    while items.len() > 1 {
        items = items
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    &chunk[0] * &chunk[1]
                } else {
                    chunk[0].clone()
                }
            })
            .collect();
    }
    items.pop().unwrap_or_else(BigInt::one)
}
