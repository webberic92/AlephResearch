use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use std::fs::File;
use std::io::Write;
use std::env;

fn main() {
    let rng = SystemRandom::new();
    let key_pair = Ed25519KeyPair::generate_pkcs8(&rng)
        .expect("failed to generate key pair");
    
    // Get node index from environment variable
    let node_index = env::var("NODE_INDEX")
        .expect("NODE_INDEX must be set")
        .parse::<u8>()
        .expect("NODE_INDEX must be an integer");

    let key_file_path = format!("/home/aleph-node/identity-{}.pem", node_index);
    
    // Save key to a file
    let mut key_file = File::create(&key_file_path)
        .expect("failed to create key file");
    key_file.write_all(key_pair.as_ref())
        .expect("failed to write key file");
    
    println!("Key saved to {}", key_file_path);
}
