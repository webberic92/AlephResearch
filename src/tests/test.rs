#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use crate::utils::{create_transaction_data::pad_to_250, merkle_utils::{compute_merkle_branch, compute_merkle_root, verify_merkle_proof}};

    fn generate_mock_tx_hashes(n: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|i| {
                let content = format!("tx_content_{}", i);
                let padded = pad_to_250(content.into_bytes());
                Sha256::digest(&padded).to_vec()
            })
            .collect()
    }

    #[test]
    fn test_merkle_proof_verification_round_trip() {
        let tx_hashes = generate_mock_tx_hashes(5); // Try different values: 3, 4, 5, 7, 8, 9

        let root = compute_merkle_root(&tx_hashes);
        assert_eq!(root.len(), 32, "Merkle root should be 32 bytes");

        for (i, leaf) in tx_hashes.iter().enumerate() {
            let proof = compute_merkle_branch(&tx_hashes, i);
            let valid = verify_merkle_proof(leaf, &proof, &root, i);

            assert!(
                valid,
                "Merkle proof failed for index {}: leaf = {:x?}, proof = {:?}, root = {:x?}",
                i,
                leaf,
                proof.iter().map(hex::encode).collect::<Vec<_>>(),
                root
            );
        }
    }

    #[test]
    fn test_merkle_proof_should_fail_on_wrong_index() {
        let tx_hashes = generate_mock_tx_hashes(4);
        let root = compute_merkle_root(&tx_hashes);

        let valid_index = 1;
        let wrong_index = 2;
        let leaf = &tx_hashes[valid_index];
        let proof = compute_merkle_branch(&tx_hashes, valid_index);

        let valid = verify_merkle_proof(leaf, &proof, &root, wrong_index);
        assert!(
            !valid,
            "Proof should fail if index is incorrect (used {}, expected {})",
            wrong_index,
            valid_index
        );
    }

    #[test]
    fn test_merkle_proof_should_fail_on_wrong_leaf() {
        let tx_hashes = generate_mock_tx_hashes(4);
        let root = compute_merkle_root(&tx_hashes);

        let valid_index = 1;
        let wrong_leaf = vec![0u8; 32]; // bogus leaf
        let proof = compute_merkle_branch(&tx_hashes, valid_index);

        let valid = verify_merkle_proof(&wrong_leaf, &proof, &root, valid_index);
        assert!(
            !valid,
            "Proof should fail if leaf is incorrect"
        );
    }
}