use aleph_research::utils::dag_utils::validate_unit;
use aleph_research::utils::dag_utils::{check_dag_sync, ensure_dag_synchronization, ensure_round_sync, get_parents, validate_unit_parents};
use mockito::mock;
use mockito::Matcher;
use reqwest::Client;
use serde_json::json;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};
use std::collections::HashSet;
use std::{collections::HashMap, sync::Arc};
use aleph_research::structs::node::Node;
use aleph_research::utils::merkle_utils::{compute_merkle_root, compute_merkle_branch};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn test_check_dag_sync_success() {
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::PartialJson(json!({
            "epoch_id": 1,
            "sender": 2
        })))
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    let client = Client::new();
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();
    let result = check_dag_sync(&client, 1, sender_id, sender_url).await;

    assert!(result.unwrap());
    mock_server.assert();
}

#[tokio::test]
async fn test_check_dag_sync_failure() {
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::PartialJson(json!({
            "epoch_id": 1,
            "sender": 2
        })))
        .with_status(500)
        .create();

    let client = Client::new();
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();
    let result = check_dag_sync(&client, 1, sender_id, sender_url).await;

    assert!(result.is_err());
    mock_server.assert();
}

#[tokio::test]
async fn test_ensure_dag_synchronization_success() {
    // Mock server response for DAG sync
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::PartialJson(json!({
            "epoch_id": 2,
            "sender": 2
        })))
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    // Create a Node instance
    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut epoch_round_id = node.epoch_round_id.lock().await;
        epoch_round_id.insert(2);
    }

    // Set up the HTTP client
    let client = Client::new();

    // Sender details
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();

    // Debugging logs
    info!("Mock Server URL: {}", sender_url);

    // Call the function being tested
    let result = ensure_dag_synchronization(&node, &client, 2, sender_id, sender_url).await;

    // Debugging output
    info!("Result of ensure_dag_synchronization: {:?}", result);

    // Assert the result is okay
    assert!(result.is_ok());

    // Verify that the mock server was called
    mock_server.assert();
}


#[tokio::test]
async fn test_ensure_dag_synchronization_failure_dag_sync() {
    let mock_server = mock("POST", "/dag_sync")
        .with_status(500)
        .create();

    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    let client = Client::new();
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();
    let result = ensure_dag_synchronization(&node, &client, 2, sender_id, sender_url).await;

    assert!(result.is_err());
    mock_server.assert();
}

#[tokio::test]
async fn test_ensure_dag_synchronization_failure_round_sync() {
    let mock_server = mock("POST", "/dag_sync")
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut epoch_round_id = node.epoch_round_id.lock().await;
        epoch_round_id.insert(1);
    }

    let client = Client::new();
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();
    let result = ensure_dag_synchronization(&node, &client, 3, sender_id, sender_url).await;

    assert!(result.is_err());
    mock_server.assert();
}

#[tokio::test]
async fn test_validate_unit_parents_success() {
    let unit = vec![1; 64];
    let parent_hashes = vec![vec![1; 32], vec![2; 32]];

    let mut dag = HashMap::new();
    for hash in &parent_hashes {
        dag.insert(hash.clone(), vec![]);
    }

    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut dag_write = node.dag.write().await;
        *dag_write = dag;
    }

    let result = validate_unit_parents(&node, &unit).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_unit_parents_failure() {
    let unit = vec![1; 64];

    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());

    let result = validate_unit_parents(&node, &unit).await;
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap(),
        "Node 1: Missing parents for unit"
    );
}

#[tokio::test]
async fn test_get_parents_success() {
    // Simulating a unit with two parent hashes (each 32 bytes)
    let mut unit = vec![0; 64]; // Mock data before the parents
    let parent1 = vec![1; 32]; // First parent hash
    let parent2 = vec![2; 32]; // Second parent hash
    unit.extend(parent1.iter()); // Add first parent hash to unit
    unit.extend(parent2.iter()); // Add second parent hash to unit

    // Call get_parents and ensure success
    let result = get_parents(&unit);
    assert!(result.is_ok());

    // Extract parents and validate
    let flat_parents = result.unwrap();
    const HASH_SIZE: usize = 32;
    let parents: Vec<Vec<u8>> = flat_parents
        .chunks(HASH_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();

    assert_eq!(parents.len(), 2);
    assert_eq!(parents[0], parent1);
    assert_eq!(parents[1], parent2);
}


#[tokio::test]
async fn test_get_parents_failure() {
    let unit = vec![0; 31];
    let result = get_parents(&unit);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap(),
        "Unit data is too small to contain parent hashes"
    );
    
}


#[tokio::test]
async fn test_validate_unit() {
    // Initialize logger
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

    // Simulated Node
    let node = Node {
        id: 1,
        total_nodes: 4,
        quorum_votes: Arc::new(RwLock::new(HashMap::new())),
        epoch_round_id: Arc::new(Mutex::new(HashSet::new())),
        finalized_blocks: Arc::new(Mutex::new(HashSet::new())),
        dag: Arc::new(RwLock::new(HashMap::new())),
        ip_address: "127.0.0.1:8001".to_string(),
        proposal_tracker: Arc::new(Mutex::new(HashSet::new())),
    };

    // Parent setup
    let parent_unit = vec![9, 8, 7, 6];
    let parent_unit_hashed = Sha256::digest(&parent_unit).to_vec();
    let parent_root = compute_merkle_root(&[parent_unit_hashed.clone()]);
    assert_eq!(
        parent_root.len(),
        32,
        "Parent root must be a 32-byte hash, but got length {}",
        parent_root.len()
    );
    node.dag.write().await.insert(parent_root.clone(), parent_unit.clone());

    // Debugging the parent root
    debug!(
        "Test setup: Inserted parent unit with root {:?}",
        parent_root
    );

    // Sample data for unit validation
    let shards = vec![vec![1; 32], vec![2; 32]];
    let root = compute_merkle_root(&shards);
    assert_eq!(
        root.len(),
        32,
        "Merkle root must be a 32-byte hash, but got length {}",
        root.len()
    );
    let proofs = compute_merkle_branch(&shards, 0);

    // Valid unit with parent hash appended
    let mut unit = vec![1, 2, 3, 4];
    unit.extend_from_slice(&parent_root); // Append the 32-byte parent root to the unit

    // Debugging the constructed unit
    debug!(
        "Constructed unit: {:?}, Length = {}",
        unit,
        unit.len()
    );

    // Test validation success
    assert!(
        validate_unit(&node, &unit, &root, &shards, &proofs).await.is_ok(),
        "Validation failed for valid unit."
    );

    // Test validation failure (missing parents)
    node.dag.write().await.clear();
    assert!(
        validate_unit(&node, &unit, &root, &shards, &proofs).await.is_err(),
        "Validation passed for unit with missing parents."
    );
}









