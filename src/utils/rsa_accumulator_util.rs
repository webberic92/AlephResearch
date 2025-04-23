use num_bigint::{BigInt, RandBigInt, Sign};
use num_traits::One;
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

//// ✅ Accumulate all elements modulo N
pub fn compute_accumulator(elements: &[Vec<u8>]) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);

    let total_product = elements.iter()
        .map(|hash| hash_to_prime(hash)) // 🚫 DO NOT rehash
        .fold(BigInt::one(), |acc, p| acc * p);

    g.modpow(&total_product, &n)
}

pub fn generate_proof(elements: &[Vec<u8>], index: usize, _accumulator: &BigInt) -> BigInt {
    let n = get_modulus();
    let g = BigInt::from(2u8);

    let product_of_others = elements.iter().enumerate()
    .filter(|(i, _)| *i != index)
    .map(|(_, hash)| hash_to_prime(hash))  // Already hashed
    .fold(BigInt::one(), |acc, p| acc * p);

    g.modpow(&product_of_others, &n)
}

pub fn generate_proofs(prime_hashes: &[Vec<u8>]) -> Vec<BigInt> {
    let primes: Vec<BigInt> = prime_hashes.iter().map(|h| hash_to_prime(h)).collect();
    let n = get_modulus();
    let g = BigInt::from(2u8);

    let mut prefix_products = vec![BigInt::one(); primes.len() + 1];
    let mut suffix_products = vec![BigInt::one(); primes.len() + 1];

    for i in 0..primes.len() {
        prefix_products[i + 1] = &prefix_products[i] * &primes[i];
    }
    for i in (0..primes.len()).rev() {
        suffix_products[i] = &suffix_products[i + 1] * &primes[i];
    }

    (0..primes.len())
        .map(|i| {
            let product = &prefix_products[i] * &suffix_products[i + 1];
            g.modpow(&product, &n)
        })
        .collect()
}



/// Verifies that a hashed element is in the RSA accumulator using its proof.
/// 
/// # Arguments
/// - `accumulator`: The RSA accumulator value (product of hashed elements).
/// - `element`: The data being proven (e.g., shard bytes).
/// - `proof`: The RSA inclusion proof (product of all other primes).
/// 
/// # Returns
/// - `true` if `proof^hash(element) == accumulator mod accumulator`, else false.
// pub fn verify_proof(accumulator: &BigInt, element: &[u8], proof: &BigInt) -> bool {
//     // 1. Hash the element (e.g., shard) using SHA256

//     // 2. Convert hash to BigInt (optional: map to a prime in real schemes)
//     // let exponent = BigInt::from_bytes_be(num_bigint::Sign::Plus, &element_hash);

//     // let exponent = crate::utils::rsa_accumulator_util::hash_to_prime(element);    
//     let hash = Sha256::digest(element).to_vec();
//     let exponent = hash_to_prime(&hash);
//     // 3. Compute proof^exponent mod accumulator
//     let reconstructed = proof.modpow(&exponent, accumulator);

//     // 4. Check that reconstructed == accumulator
//     &reconstructed == accumulator
// }


// pub fn verify_proof(accumulator: &BigInt, element: &[u8], proof: &BigInt) -> bool {
//     let exponent = hash_to_prime(element);  // not hash_to_prime(Sha256::digest(...))
//     let reconstructed = proof.modpow(&exponent, accumulator);
//     &reconstructed == accumulator
// }

// pub fn verify_proof(accumulator: &BigInt, element: &[u8], proof: &BigInt) -> bool {
//     let hash = Sha256::digest(element).to_vec();
//     let exponent = hash_to_prime(&hash);
//     let reconstructed = proof.modpow(&exponent, accumulator);
//     &reconstructed == accumulator
// }


pub fn verify_proof(accumulator: &BigInt, shard: &[u8], proof: &BigInt) -> bool {
    let hash = Sha256::digest(shard).to_vec();          // ✅ Hash first
    let prime = hash_to_prime(&hash);                   // ✅ Then map to prime
    let reconstructed = proof.modpow(&prime, &get_modulus()); // ✅ Modulo N

    let is_valid = &reconstructed == accumulator;
    if !is_valid {
        println!("❌ verify_proof failed: hash={}, prime={}, proof={}, reconstructed={}, acc={}",
            hex::encode(&hash),
            prime.to_str_radix(10).chars().take(12).collect::<String>(),
            proof.to_str_radix(10).chars().take(12).collect::<String>(),
            reconstructed.to_str_radix(10).chars().take(12).collect::<String>(),
            accumulator.to_str_radix(10).chars().take(12).collect::<String>(),
        );
    }

    is_valid
}
