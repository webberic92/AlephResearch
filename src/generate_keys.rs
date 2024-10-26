use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::fs::File;
use std::io::Write;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "Failed to generate pkcs8 key")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).map_err(|_| "Failed to parse pkcs8 key")?;

    // Save private key
    let mut private_key_file = File::create("private_key.pkcs8")?;
    private_key_file.write_all(pkcs8_bytes.as_ref())?;

    // Save public key
    let mut public_key_file = File::create("public_key.der")?;
    public_key_file.write_all(key_pair.public_key().as_ref())?;

    Ok(())
}
