use aleph_research::utils::dag_utils::{are_parents_available, ensure_round_sync, get_parent_hashes};
use aleph_research::utils::dag_utils::{check_dag_sync, ensure_dag_synchronization, validate_unit_parents};
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
use aleph_research::utils::merkle_utils::{compute_merkle_branch, compute_merkle_root, validate_merkle_branch};
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
    )));

    {
        let mut node_state = node.write().await;
        let mut epoch_round_id = node_state.epoch_round_id.lock().await;
        epoch_round_id.insert(2);
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
    let mut unit = vec![1];
    unit.extend(vec![1; 32]);

    let dag = HashMap::new();

    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
    )));
    {
        let  node_write = node.write().await; // Acquire a write lock on the node
        let mut dag_write = node_write.dag.write().await; // Access the `dag` field and acquire a write lock
        *dag_write = dag; // Assign the new DAG value
    }

    let result = validate_unit_parents(node.clone(), &unit).await;

    assert!(
        result.is_err(),
        "Validation unexpectedly succeeded for unit: {:?}",
        unit
    );

    let expected_error = format!(
        "Node {}: Parent unit {:?} not committed in DAG.",
        1,
        vec![1; 32]
    );
    assert_eq!(
        result.unwrap_err(),
        expected_error,
        "Unexpected error message"
    );
}




#[tokio::test]
async fn test_get_parents_success() {
    // Create a unit with a parent count and parent hashes
    let mut unit = vec![2]; // Parent count: 2
    let parent1 = vec![1; 32]; // First parent hash
    let parent2 = vec![2; 32]; // Second parent hash
    unit.extend(&parent1); // Add first parent hash
    unit.extend(&parent2); // Add second parent hash

    // Call get_parent_hashes and ensure success
    let result = get_parent_hashes(&unit);
    assert!(result.is_ok(), "get_parent_hashes failed with result: {:?}", result);

    // Extract parents from the result
    let parents = result.unwrap();

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
    let result = get_parent_hashes(&unit);
    assert!(result.is_err(), "get_parents unexpectedly succeeded");

    // Validate the error message
    assert_eq!(
        result.unwrap_err(),
        "Unit data too short to contain all parent hashes",
        "Unexpected error message"
    );
}



// #[tokio::test]
// async fn test_validate_unit() {
//     tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();

//     let node = Arc::new(RwLock::new(Node::new(
//         1,
//         4,
//         "127.0.0.1:8001".to_string(),
//         vec!["127.0.0.1:8002".to_string()],
//     )));

//     let parent_unit = vec![9, 8, 7, 6];
//     let parent_unit_hashed = Sha256::digest(&parent_unit).to_vec();
//     let parent_root = compute_merkle_root(&[parent_unit_hashed.clone()]);
//     assert_eq!(
//         parent_root.len(),
//         32,
//         "Parent root must be a 32-byte hash, but got length {}",
//         parent_root.len()
//     );

//     {
//         let mut dag_write = node.write().await.dag.write().await;
//         dag_write.insert(parent_root.clone(), parent_unit.clone());
//     }

//     let shards = vec![vec![1; 32], vec![2; 32]];
//     let root = compute_merkle_root(&shards);
//     let proofs = compute_merkle_branch(&shards, 0);

//     let mut unit = vec![5, 6, 7, 8];
//     unit.extend_from_slice(&parent_root);

//     let valid_result = validate_unit(node.clone(), &unit, &root, &shards, &proofs).await;
//     assert!(
//         valid_result.is_ok(),
//         "Validation failed for valid unit. Error: {:?}",
//         valid_result.err()
//     );

//     {
//         let mut dag_write = node.write().await.dag.write().await;
//         dag_write.clear();
//     }

//     let invalid_result = validate_unit(node.clone(), &unit, &root, &shards, &proofs).await;
//     assert!(invalid_result.is_err(), "Validation unexpectedly passed");
//     assert_eq!(
//         invalid_result.unwrap_err(),
//         format!(
//             "Node {}: Parent availability check failed for unit {:?}",
//             node.read().await.id,
//             unit
//         )
//     );
// }

#[tokio::test]
async fn test_ensure_round_sync_success() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
    )));

    {
        let node_write = node.write().await; // Acquire a write lock on the node
        let mut epoch_round_id = node_write.epoch_round_id.lock().await; // Access `epoch_round_id` and acquire a lock
        epoch_round_id.insert(2); // Perform the insertion
    }

    let result = ensure_round_sync(node.clone(), 3).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_ensure_round_sync_failure() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
    )));

    {
        let  node_write = node.write().await; // Acquire a write lock on the node
        let mut epoch_round_id = node_write.epoch_round_id.lock().await; // Access `epoch_round_id` and acquire a lock
        epoch_round_id.insert(2); // Perform the insertion
    }

    let result = ensure_round_sync(node.clone(), 3).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Node 1: DAG not synchronized to round 2 for prevote (current round: 1)"
    );
}

#[tokio::test]
async fn test_are_parents_available_success() {
    let node = Arc::new(RwLock::new(Node::new(
        1,
        4,
        "127.0.0.1:8001".to_string(),
        vec!["127.0.0.1:8002".to_string()],
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

