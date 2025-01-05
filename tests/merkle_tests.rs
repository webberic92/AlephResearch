#[cfg(test)]
mod merkle_tests {
    use sha2::{Digest, Sha256};
    use AlephResearch_Original::alephStart::{compute_merkle_root, compute_merkle_branch, Node};

    fn generate_test_shards(shard_count: usize, shard_size: usize) -> Vec<Vec<u8>> {
        (0..shard_count)
            .map(|i| vec![i as u8; shard_size])
            .collect()
    }

    #[test]
    fn test_merkle_root_computation() {
        let shards = generate_test_shards(4, 256);
        let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
        let root = compute_merkle_root(&shard_hashes);

        assert!(!root.is_empty(), "Merkle root should not be empty");
    }

    #[test]
    fn test_merkle_branch_validation() {
        let shards = generate_test_shards(4, 256);
        let shard_hashes: Vec<Vec<u8>> = shards.iter().map(|s| Sha256::digest(s).to_vec()).collect();
        let root = compute_merkle_root(&shard_hashes);

        for (index, shard) in shards.iter().enumerate() {
            let branch = compute_merkle_branch(&shard_hashes, index);
            let computed_root = Node::validate_merkle_branch(shard, &branch);

            assert_eq!(computed_root, root, "Merkle root mismatch for shard index {}", index);
        }
    }
}
