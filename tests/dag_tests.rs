use aleph_research::utils::dag_utils::validate_unit;
use aleph_research::utils::dag_utils::{check_dag_sync, ensure_dag_synchronization, get_parents, validate_unit_parents};
use base64::Engine;
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
        .match_body(Matcher::Json(json!({
            "epoch_id": 1,
            "sender_id": 2,
            "sender_url": mockito::server_url()
        })))
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    let client = Client::new();
    let sender_id = 2usize;
    let sender_url = mockito::server_url(); // Match the mock server URL
    let result = check_dag_sync(&client, 1, &sender_id, &sender_url).await;

    assert!(result.unwrap());
    mock_server.assert();
}



#[tokio::test]
async fn test_check_dag_sync_failure() {
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::Json(json!({
            "epoch_id": 1,
            "sender_id": 2,
            "sender_url": mockito::server_url()
        })))
        .with_status(500)
        .create();

    let client = Client::new();
    let sender_id = 2usize;
    let sender_url = mockito::server_url(); // Match the mock server's base URL
    let result = check_dag_sync(&client, 1, &sender_id, &sender_url).await;

    // Assert that the function returns an error
    assert!(result.is_err());
    mock_server.assert();
}


#[tokio::test]
async fn test_ensure_dag_synchronization_success() {
    // Mock server response for DAG sync
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::Json(json!({
            "epoch_id": 2,
            "sender_id": 2,
            "sender_url": mockito::server_url()
        })))
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    // Create a Node instance
    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut epoch_round_id = node.epoch_round_id.lock().await;
        epoch_round_id.insert(2); // Ensure the epoch is already being tracked
    }

    // Set up the HTTP client
    let client = Client::new();

    // Sender details
    let sender_id = 2usize;
    let sender_url = mockito::server_url(); // Use the mock server's URL

    // Debugging logs
    info!("Mock Server URL: {}", sender_url);

    // Call the function being tested
    let result = ensure_dag_synchronization(&node, &client, 2, &sender_id, &sender_url).await;

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
    // Create a unit with parent count and parent hashes
    let mut unit = vec![2]; // Parent count: 2
    unit.extend(vec![1; 32]); // First parent hash
    unit.extend(vec![2; 32]); // Second parent hash

    // Prepare the DAG and finalized blocks
    let mut dag = HashMap::new();
    dag.insert(vec![1; 32], vec![]); // First parent
    dag.insert(vec![2; 32], vec![]); // Second parent

    let mut finalized_blocks = HashSet::new();
    finalized_blocks.insert(vec![1; 32]); // First parent finalized
    finalized_blocks.insert(vec![2; 32]); // Second parent finalized

    // Create a Node and populate its DAG and finalized blocks
    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut dag_write = node.dag.write().await;
        *dag_write = dag;
    }
    {
        let mut finalized_write = node.finalized_blocks.lock().await;
        *finalized_write = finalized_blocks;
    }

    // Call the method and assert success
    let result = validate_unit_parents(&node, &unit).await;
    assert!(result.is_ok(), "Validation failed with result: {:?}", result);
}




#[tokio::test]
async fn test_validate_unit_parents_failure() {
    // Create a unit with parent count and parent hash
    let mut unit = vec![1]; // Parent count: 1
    unit.extend(vec![1; 32]); // First parent hash

    // Prepare an empty DAG and finalized blocks
    let dag = HashMap::new(); // No parents in the DAG
    let finalized_blocks = HashSet::new(); // No finalized blocks

    // Create a Node and populate its DAG and finalized blocks
    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    {
        let mut dag_write = node.dag.write().await;
        *dag_write = dag;
    }
    {
        let mut finalized_write = node.finalized_blocks.lock().await;
        *finalized_write = finalized_blocks;
    }

    // Call the method and assert failure
    let result = validate_unit_parents(&node, &unit).await;
    assert!(result.is_err(), "Validation unexpectedly succeeded");

    // Validate the error message
    let expected_error = format!(
        "Parent unit {} not committed for root {:?}",
        base64::engine::general_purpose::STANDARD.encode(vec![1; 32]), // Encoded parent hash
        base64::engine::general_purpose::STANDARD.encode(&unit),       // Encoded unit root
    );
    assert_eq!(result.unwrap_err(), expected_error);
}



#[tokio::test]
async fn test_get_parents_success() {
    // Create a unit with a parent count and parent hashes
    let mut unit = vec![2]; // Parent count: 2
    let parent1 = vec![1; 32]; // First parent hash
    let parent2 = vec![2; 32]; // Second parent hash
    unit.extend(&parent1); // Add first parent hash
    unit.extend(&parent2); // Add second parent hash

    // Call get_parents and ensure success
    let result = get_parents(&unit);
    assert!(result.is_ok(), "get_parents failed with result: {:?}", result);

    // Extract parents and validate
    let flat_parents = result.unwrap();
    const HASH_SIZE: usize = 32;
    let parents: Vec<Vec<u8>> = flat_parents
        .chunks(HASH_SIZE)
        .map(|chunk| chunk.to_vec())
        .collect();

    // Ensure the number of parents and their values are correct
    assert_eq!(parents.len(), 2, "Unexpected number of parents");
    assert_eq!(parents[0], parent1, "First parent hash mismatch");
    assert_eq!(parents[1], parent2, "Second parent hash mismatch");
}



#[tokio::test]
async fn test_get_parents_failure() {
    // Create a unit with an invalid size (less than needed for parent count and hashes)
    let unit = vec![1]; // Indicates 1 parent but no space for the hash

    // Call get_parents and ensure it fails
    let result = get_parents(&unit);
    assert!(result.is_err(), "get_parents unexpectedly succeeded");

    // Validate the error message
    assert_eq!(
        result.unwrap_err(),
        "Unit data too short to contain all parent hashes",
        "Unexpected error message"
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

    {
        let mut dag_write = node.dag.write().await;
        dag_write.insert(parent_root.clone(), parent_unit.clone());
    }

    info!(
        "Test setup: Inserted parent unit with root {:?} into DAG",
        parent_root
    );

    // Shard data for Merkle root computation
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
    let mut unit = vec![5, 6, 7, 8]; // Example unit data
    unit.extend_from_slice(&parent_root); // Append the 32-byte parent root to the unit

    info!("Constructed valid unit: {:?}", unit);

    // Test validation success
    let valid_result = validate_unit(&node, &unit, &root, &shards, &proofs).await;
    assert!(
        valid_result.is_ok(),
        "Validation failed for valid unit. Error: {:?}",
        valid_result.err()
    );

    // Clear the DAG to simulate missing parents
    {
        let mut dag_write = node.dag.write().await;
        dag_write.clear();
    }

    // Test validation failure
    let invalid_result = validate_unit(&node, &unit, &root, &shards, &proofs).await;
    assert!(
        invalid_result.is_err(),
        "Validation unexpectedly passed for unit with missing parents"
    );

    assert_eq!(
        invalid_result.unwrap_err(),
        format!(
            "Node {}: Parent availability check failed for unit {:?}",
            node.id, unit
        )
    );
}











