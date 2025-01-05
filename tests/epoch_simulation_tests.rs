#[tokio::test]
async fn test_full_epoch_flow() {


    use AlephResearch_Original::alephStart::{load_config, generate_and_send_transactions_in_order};
    use AlephResearch_Original::alephRBC::Node;
    use reqwest::Client;

    let config = load_config("test_config.toml");
    let client = Client::new();
    let node = Node::new(config.node.id, config.node.total_nodes);

    // Simulate generating and sending transactions
    let epoch_id = 1;
    generate_and_send_transactions_in_order(&client, &config, &node, epoch_id)
        .await
        .expect("Failed to complete transaction generation");

    // Simulate receiving proposals from all nodes
    for sender_id in 1..=config.node.total_nodes {
        node.handle_propose(
            &client,
            &config,
            sender_id,
            vec![0u8; 32], // Dummy Merkle root
            vec![vec![0u8; 32]; 2], // Dummy proof
            vec![0u8; 256], // Dummy shard
            epoch_id,
        )
        .await;
    }

    // Ensure all proposals were collected
    let proposal_tracker = node.proposal_tracker.lock().await;
    assert_eq!(
        proposal_tracker.len(),
        config.node.total_nodes,
        "Not all proposals were received"
    );

    // Ensure prevote phase is triggered (add more assertions for prevote/commit phases)
}
