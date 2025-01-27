
#[cfg(test)]
mod tests {
    
    use aleph_research::utils::epoch_utils::{update_epoch_to_next_round, ensure_no_overlap, handle_sync_epoch};
    use aleph_research::structs::{node::Node, toml_config::TomlConfig};
    use reqwest::ClientBuilder;
    use std::sync::{Arc, Mutex};
    use tokio::sync::Mutex as AsyncMutex;
    use std::{collections::HashSet};
    
    use aleph_research::{ utils::{config_util::load_config}};
    
    #[tokio::test]
    async fn test_ensure_no_overlap() {
        let epoch_tracker = Arc::new(AsyncMutex::new(HashSet::new()));
        let node = Node {
            id: 1,
            epoch_round_id: epoch_tracker.clone(),
            total_nodes: 3,
            quorum_votes: Default::default(),
            proposal_tracker: Default::default(),
            finalized_blocks: Default::default(),
            dag: Default::default(),
            ip_address: "127.0.0.1".to_string(),
        };
    
        // Add an epoch and ensure no error
        assert!(ensure_no_overlap(&node, 1).await.is_ok());
        assert!(node.epoch_round_id.lock().await.contains(&1));
    
        // Add the same epoch and ensure no error (overlap detection)
        assert!(ensure_no_overlap(&node, 1).await.is_ok());
    }
    

    #[tokio::test]
    async fn test_update_epoch_to_next_round() {
        let temp_config_path = "test_config_update_epoch.toml"; // Unique file path for this test
        let config_content = r#"
            [network]
            listen_address = "127.0.0.1"
            nodes = ["http://node1.local", "http://node2.local"]
            ip_manager_address = "http://ip_manager.local"
            proposals = [1, 2, 3]
            ip_address = "127.0.0.1"
    
            [consensus]
            transaction_size = 256
            data_shards = 4
            batch_size = 10
            epoch_round_id = 1
    
            [node]
            id = 1
            total_nodes = 3
        "#;
    
        std::fs::write(temp_config_path, config_content).expect("Failed to write temporary config");
    
        let client = Arc::new(ClientBuilder::new().build().unwrap());
        update_epoch_to_next_round(&client.clone(), Some(temp_config_path)).await;
    
        std::fs::remove_file(temp_config_path).expect("Failed to clean up test config");
    }
    
    #[tokio::test]
    async fn test_integration_epoch_functions() {
        let temp_config_path = "test_config_integration_epoch.toml"; // Unique file path for this test
        let config_content = r#"
            [network]
            listen_address = "127.0.0.1"
            nodes = ["http://node1.local", "http://node2.local"]
            ip_manager_address = "http://ip_manager.local"
            proposals = [1, 2, 3]
            ip_address = "127.0.0.1"
    
            [consensus]
            transaction_size = 256
            data_shards = 4
            batch_size = 10
            epoch_round_id = 5
    
            [node]
            id = 1
            total_nodes = 3
        "#;
    
        std::fs::write(temp_config_path, config_content).expect("Failed to write temporary config");
    
        let client = Arc::new(ClientBuilder::new().build().unwrap());
        let epoch_tracker = Arc::new(AsyncMutex::new(HashSet::new()));
        let node = Node {
            id: 1,
            epoch_round_id: epoch_tracker.clone(),
            total_nodes: 3,
            quorum_votes: Default::default(),
            proposal_tracker: Default::default(),
            finalized_blocks: Default::default(),
            dag: Default::default(),
            ip_address: "127.0.0.1".to_string(),
        };
    
        let toml_config = load_config(Some(temp_config_path));
        update_epoch_to_next_round(&client.clone(), Some(temp_config_path)).await;
    
        let new_epoch_id = toml_config.consensus.epoch_round_id + 1;
        assert_eq!(new_epoch_id, 6);
    
        let is_epoch_tracked = epoch_tracker.lock().await.contains(&new_epoch_id);
        assert!(!is_epoch_tracked, "Epoch tracking failed to update.");
    
        std::fs::remove_file(temp_config_path).expect("Failed to clean up test config");
    }
    
}
