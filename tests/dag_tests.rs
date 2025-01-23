use aleph_research::utils::dag_utils::{check_dag_sync, ensure_dag_synchronization, ensure_round_sync, get_parents, validate_unit_parents};
use mockito::mock;
use mockito::Matcher;
use reqwest::Client;
use serde_json::json;
use tokio::sync::RwLock;
use std::{collections::HashMap, sync::Arc};
use aleph_research::structs::node::Node;

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
    let mock_server = mock("POST", "/dag_sync")
        .with_status(200)
        .with_body(r#"{"in_sync": true}"#)
        .create();

    let node = Node::new(1, 4, "127.0.0.1:8001".to_string());
    let client = Client::new();
    let sender_id = &2usize;
    let sender_url = &mockito::server_url();
    let result = ensure_dag_synchronization(&node, &client, 2, sender_id, sender_url).await;

    assert!(result.is_ok());
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
    let unit = vec![0; 96];
    let parent1 = vec![1; 32];
    let parent2 = vec![2; 32];
    let mut unit_with_parents = unit.clone();
    unit_with_parents.splice(64..96, parent1.iter().chain(parent2.iter()).cloned());

    let result = get_parents(&unit_with_parents);
    assert!(result.is_ok());
    let parents = result.unwrap();
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
