#[cfg(test)]
mod tests {
    use aleph_research::{structs::node::Node, utils::round_utils::{broadcast_round_update, update_local_round}};
    use mockito::{mock, server_url, Matcher};
    use reqwest::Client;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    

    #[tokio::test]
    async fn test_update_local_round_success() {
        let node = Arc::new(RwLock::new(Node::new(
            1, 
            4, 
            "127.0.0.1:8001".to_string(),
            vec!["127.0.0.1:8002".to_string()],
            "127.0.0.1:8003".to_string(),

        )));

        // Initial round
        {
            let node_read = node.read().await;
            let initial_round = *node_read.current_round.lock().await;
            assert_eq!(initial_round, 0);
        }

        let new_round = update_local_round(node.clone()).await;

        // Check updated round
        {
            let node_read = node.read().await;
            let updated_round = *node_read.current_round.lock().await;
            assert_eq!(updated_round, 1);
            assert!(*node_read.current_round.lock().await == 1);
        }

        assert_eq!(new_round, 1);
    }

    #[tokio::test]
    async fn test_broadcast_round_update_success() {
        // Mock server setup
        let _mock = mock("POST", "/sync_round")
            .match_body(Matcher::Any)
            .with_status(200) // Simulate success
            .with_body(r#"{"status":"round synchronized"}"#)
            .create();

        let client = Arc::new(Client::new());
        let node = Arc::new(RwLock::new(Node::new(
            1, 
            4, 
            "127.0.0.1:8001".to_string(),
            vec![server_url()],
            "127.0.0.1:8003".to_string(),

        )));

        let round_id = 1;
        let result = broadcast_round_update(node.clone(), client.clone(), round_id).await;

        assert!(
            result.is_ok(),
            "Expected Ok, but got Err: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_broadcast_round_update_failure() {
        // Mock server returns a 500 Internal Server Error
        let _mock = mock("POST", "/sync_round")
            .match_body(Matcher::Any)
            .with_status(500)
            .with_body(r#"{"status":"Internal Server Error"}"#)
            .create();

        let client = Arc::new(Client::new());
        let node = Arc::new(RwLock::new(Node::new(
            1, 
            4, 
            "127.0.0.1:8001".to_string(),
            vec![server_url()],
            "127.0.0.1:8003".to_string(),

        )));

        let round_id = 1;
        let result = broadcast_round_update(node.clone(), client.clone(), round_id).await;

        assert!(
            result.is_err(),
            "Expected Err but got Ok()"
        );
    }

    #[tokio::test]
    async fn test_broadcast_round_update_network_error() {
        let invalid_url = "http://invalid_url";
        
        let client = Arc::new(Client::new());
        let node = Arc::new(RwLock::new(Node::new(
            1, 
            4, 
            "127.0.0.1:8001".to_string(),
            vec![invalid_url.to_string()],
            "127.0.0.1:8003".to_string(),

        )));

        let round_id = 1;
        let result = broadcast_round_update(node.clone(), client.clone(), round_id).await;

        assert!(result.is_err(), "Expected Err but got Ok()");
    }
}
