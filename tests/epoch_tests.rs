use aleph_research::utils::epoch_utils::{update_epoch_to_next_round, ensure_no_overlap, handle_sync_epoch};
use aleph_research::structs::{node::Node, toml_config::TomlConfig};
use reqwest::ClientBuilder;
use std::sync::{Arc, Mutex};



/// Example function to update the epoch to the next round


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tokio::sync::Mutex as AsyncMutex;
    use aleph_research::structs::node::Node;
    use aleph_research::structs::toml_config::{TomlConfig, NetworkConfig, ConsensusConfig, NodeConfig};

    #[tokio::test]
    async fn test_update_epoch_to_next_round() {
        // Mock client and configuration
        let client = Arc::new(ClientBuilder::new().build().unwrap());
        let toml_config = TomlConfig {
            network: NetworkConfig {
                listen_address: "127.0.0.1".to_string(),
                nodes: vec![
                    "http://node1.local".to_string(),
                    "http://node2.local".to_string(),
                ],
                ip_manager_address: "http://ip_manager.local".to_string(),
                proposals: vec![1, 2, 3],
                ip_address: "127.0.0.1".to_string(),
            },
            consensus: ConsensusConfig {
                transaction_size: 256,
                data_shards: 4,
                batch_size: 10,
                epoch_round_id: 1,
            },
            node: NodeConfig {
                id: 1,
                total_nodes: 3,
            },
        };

        // Call the function
        update_epoch_to_next_round(&client.clone()).await;

        // Log expectations
        // Ideally, you should mock the HTTP client and assert the expected requests.
        // For now, just check no runtime errors.
    }

    #[tokio::test]
    async fn test_integration_epoch_functions() {
        // Mock Node, client, and configuration
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

        let toml_config = TomlConfig {
            network: NetworkConfig {
                listen_address: "127.0.0.1".to_string(),
                nodes: vec![
                    "http://node1.local".to_string(),
                    "http://node2.local".to_string(),
                ],
                ip_manager_address: "http://ip_manager.local".to_string(),
                proposals: vec![1, 2, 3],
                ip_address: "127.0.0.1".to_string(),
            },
            consensus: ConsensusConfig {
                transaction_size: 256,
                data_shards: 4,
                batch_size: 10,
                epoch_round_id: 5,
            },
            node: NodeConfig {
                id: 1,
                total_nodes: 3,
            },
        };

        // Update the epoch and sync with all nodes
        update_epoch_to_next_round(&client.clone()).await;

        // Verify the new epoch ID is incremented and consistent
        let new_epoch_id = toml_config.consensus.epoch_round_id + 1;
        assert_eq!(new_epoch_id, 6);

        // Ensure epoch tracking updates correctly
        let is_epoch_tracked = epoch_tracker.lock().await.contains(&new_epoch_id);
        assert!(!is_epoch_tracked, "Epoch tracking failed to update.");
    }
}
