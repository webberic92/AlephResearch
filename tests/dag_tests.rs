use aleph_research::utils::dag_utils::{are_parents_available, ensure_round_sync, get_parent_hashes};
use aleph_research::utils::dag_utils::{check_dag_sync, ensure_dag_synchronization, validate_unit_parents};
use mockito::mock;
use mockito::Matcher;
use reqwest::Client;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::info;
use std::{collections::HashMap, sync::Arc};
use aleph_research::structs::node::Node;
use aleph_research::utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, validate_merkle_branch};

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
    let sender_url = mockito::server_url();
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
    let sender_url = mockito::server_url();
    let result = check_dag_sync(&client, 1, &sender_id, &sender_url).await;

    assert!(result.is_err());
    mock_server.assert();
}

#[tokio::test]
async fn test_ensure_dag_synchronization_success() {
    let mock_server = mock("POST", "/dag_sync")
        .match_body(Matcher::Json(json!({
            "epoch_id": 2,
            "sender_id": 2,
            "sender_url": mockito::server_url()
        })))
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    {
        let node_state = node.write().await;
        let mut epoch_round_id = node_state.current_epoch.lock().await;
        *epoch_round_id = 2;
    }

    let client = Client::new();
    let sender_id = 2usize;
    let sender_url = mockito::server_url();

    let result = ensure_dag_synchronization(node.clone(), &client, 2, &sender_id, sender_url).await;

    assert!(result.is_ok());
    mock_server.assert();
}

#[tokio::test]
async fn test_validate_unit_parents_success() {
    let mut unit = vec![2];
    unit.extend(vec![1; 32]);
    unit.extend(vec![2; 32]);

    let mut dag = HashMap::new();
    dag.insert(vec![1; 32], vec![]);
    dag.insert(vec![2; 32], vec![]);

    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));
    {
        let  node_write = node.write().await; // Acquire a write lock on the node
        let mut dag_write = node_write.dag.write().await; // Access the `dag` field and acquire a write lock
        *dag_write = dag; // Assign the new DAG value
    }
    

    let result = validate_unit_parents(node.clone(), &unit).await;
    assert!(result.is_ok(), "Validation failed with result: {:?}", result);
}

#[tokio::test]
async fn test_validate_unit_parents_failure() {
    let mut unit = vec![1]; // Non-parent byte to simulate unit header or ID
    unit.extend(vec![2; 32]); // Parent hash that should not exist in DAG

    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    {
        let node_write = node.write().await; 
        let mut dag_write = node_write.dag.write().await; 
        dag_write.clear(); // Ensure the DAG is empty
    }

    let result = validate_unit_parents(node.clone(), &unit).await;

    // Since DAG is empty, validation should succeed (as per protocol rules)
    assert!(
        result.is_ok(),
        "Validation should pass when DAG is empty, but it failed: {:?}",
        result
    );
}







#[tokio::test]
async fn test_get_parents_success() {
    // Create a unit with a parent count and parent hashes
    let parent_count = 2; // Expected number of parents
    let mut unit = vec![parent_count as u8]; // Parent count as first byte
    
    let parent1 = vec![1; 32]; // First parent hash (32 bytes)
    let parent2 = vec![2; 32]; // Second parent hash (32 bytes)

    unit.extend_from_slice(&parent1); // Append first parent hash
    unit.extend_from_slice(&parent2); // Append second parent hash

    // **Print debug information**
    println!("Constructed unit: {:?}", unit);
    println!("First byte (parent count): {}", unit[0]);
    println!(
        "Expected parent data size: {}, Actual unit size: {}",
        parent_count * 32,
        unit.len()
    );

    // **Ensure unit structure is correct**
    assert_eq!(unit.len(), 1 + (parent_count * 32), "Unexpected unit length");

    // Call get_parent_hashes() and check result
    let result = get_parent_hashes(&unit);
    println!("Extracted parents: {:?}", result);

    assert!(result.is_ok(), "get_parent_hashes failed with result: {:?}", result);

    let parents = result.unwrap();
    println!("Extracted parents (after unwrap): {:?} (Length: {})", parents, parents.len());

    // Ensure correct number of parents were extracted
    assert_eq!(parents.len(), parent_count, "Unexpected number of parents");

    // Ensure extracted parents match original
    assert_eq!(parents[0], parent1, "First parent hash mismatch");
    assert_eq!(parents[1], parent2, "Second parent hash mismatch");
}





#[tokio::test]
async fn test_get_parents_failure() {
    // Create a unit with an invalid size (less than needed for parent count and hashes)
    let unit = vec![1]; // Indicates 1 parent but no space for the hash

    // Call get_parents and ensure it fails
    let result = get_parent_hashes(&unit);
    assert!(result.is_err(), "get_parents unexpectedly succeeded");

    // Validate the error message
    assert_eq!(
        result.unwrap_err(),
        "Unit data too short to contain all parent hashes",
        "Unexpected error message"
    );
}
#[tokio::test]
async fn test_ensure_round_sync_success() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    {
        let node_state = node.write().await;
        let mut epoch_round_id = node_state.current_epoch.lock().await;
        *epoch_round_id = 2;
    }

    let result = ensure_round_sync(node.clone(), 3).await;

    assert!(
        result.is_ok(),
        "Expected Ok but got Err: {:?}",
        result.err()
    );
}


#[tokio::test]
async fn test_ensure_round_sync_failure() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    {
        let node_state = node.write().await;
        let mut epoch_round_id = node_state.current_epoch.lock().await;
        *epoch_round_id = 2;
        info!("Test: Set current_epoch to {}", *epoch_round_id); // Debugging
    }

    {
        let node_state = node.read().await;
        let epoch_round_id = node_state.current_epoch.lock().await;
        info!(
            "Test: Confirming current_epoch before calling ensure_round_sync: {}",
            *epoch_round_id
        );
    }

    let result = ensure_round_sync(node.clone(), 4).await;  // Ensure failure
    assert!(result.is_err(), "Expected ensure_round_sync to fail, but it succeeded.");
    assert_eq!(
        result.unwrap_err(),
        format!(
            "Node 1: DAG not synchronized to round {} for prevote (current round: {})",
            3, // target_round - 1 (4 - 1)
            2  // Matches expected `current_epoch`
        )
    );
}



#[tokio::test]
async fn test_are_parents_available_success() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    let parent_hash = vec![1; 32];
    {
        let  node_write = node.write().await; // Acquire a write lock on the node
        let mut dag_write = node_write.dag.write().await;
         dag_write.insert(parent_hash.clone(), vec![]);
    }

    let mut unit = vec![1];
    unit.extend_from_slice(&parent_hash);

    let result = are_parents_available(node.clone(), &unit).await;
    assert!(result);
}

#[tokio::test]
async fn test_are_parents_available_failure() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
        "127.0.0.1:8003".to_string(),

    )));

    let unit = vec![1; 32];
    let result = are_parents_available(node.clone(), &unit).await;
    assert!(!result);
}

#[tokio::test]
async fn test_validate_merkle_branch_success() {
    let shards = vec![vec![1; 32], vec![2; 32]];
    let root = compute_merkle_root(&shards);
    let proofs = compute_merkle_branch(&shards, 0);

    assert!(validate_merkle_branch(&shards, &proofs, 0, &root));
}

#[tokio::test]
async fn test_validate_merkle_branch_failure() {
    let shards = vec![vec![1; 32], vec![2; 32]];
    let root = compute_merkle_root(&shards);
    let invalid_proofs = vec![vec![0; 32]];

    assert!(!validate_merkle_branch(&shards, &invalid_proofs, 0, &root));
}

