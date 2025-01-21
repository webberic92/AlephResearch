use reqwest::Client;
use tracing::{error, info};

use crate::{structs::{requests::PrevoteRequest, toml_config::TomlConfig}, utils::merkle_utils::compute_merkle_branch};
pub async fn send_prevotes(
    client: &Client,
    toml_config: &TomlConfig,
    merkle_root: &Vec<u8>,
    proofs: &Vec<Vec<u8>>, // Updated to plural
    shards: &Vec<Vec<u8>>, // Updated to plural
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Node {} {}: Starting PREVOTE phase from aleph_start...",
        toml_config.node.id, toml_config.network.ip_address
    );

    let mut all_successful = true; // Track if all prevote messages succeed

    // Multicast prevote message to all nodes
    for (index, node_url) in toml_config.network.nodes.iter().enumerate() {
        let shard = &shards[index % shards.len()];
        let merkle_branch: Vec<Vec<u8>> = compute_merkle_branch(&proofs, index % proofs.len())
            .iter()
            .map(|hash| hash.to_vec())
            .collect();

        // Construct prevote payload
        let payload = PrevoteRequest {
            sender: toml_config.node.id,
            epoch_id: toml_config.consensus.epoch_round_id,
            root: merkle_root.clone(),         // Raw byte array
            proofs: vec![merkle_branch],       // Raw byte arrays as Vec<Vec<Vec<u8>>>
            shards: vec![shard.clone()],       // Raw byte arrays
            node_url: node_url.to_string(),    // Include the node URL
        };

        info!(
            "Node {}: Sending prevote to {}. Payload: {:?}",
            toml_config.node.id, node_url, payload
        );

        let url = format!("http://{}/prevote", node_url);
        let response = client.post(&url).json(&payload).send().await;

        match response {
            Ok(res) => {
                let status = res.status();
                let response_body = res.text().await.unwrap_or_else(|_| "Failed to read response body".to_string());

                if status.is_success() {
                    info!(
                        "Node {}: Prevote successfully sent to {}. Response: {}",
                        toml_config.node.id, node_url, response_body
                    );
                } else {
                    error!(
                        "Node {}: Failed to send prevote to {}. Status: {}. Response: {}",
                        toml_config.node.id, node_url, status, response_body
                    );
                    all_successful = false; // Mark failure if any message fails
                }
            }
            Err(e) => {
                error!(
                    "Node {}: Error sending prevote to {}: {:?}",
                    toml_config.node.id, node_url, e
                );
                all_successful = false; // Mark failure if there's a network error
            }
        }
    }

    // Final log and return result based on success/failure
    if all_successful {
        info!(
            "Node {} {}: All PREVOTE messages sent successfully.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Ok(())
    } else {
        error!(
            "Node {} {}: Failed to send one or more PREVOTE messages.",
            toml_config.node.id, toml_config.network.ip_address
        );
        Err("One or more PREVOTE messages failed".into())
    }
}





// async fn send_prevotes(
//     node: &Node,
//     client: &Client,
//     epoch_id: u64,
//     root: &Vec<u8>,
//     proofs: &Vec<Vec<Vec<u8>>>,
//     shards: &Vec<Vec<u8>>,
// ) {
//     info!(
//         "Node {} {} All Proposals received: Finalizing epoch {}",
//         node.id, node.ip_address, epoch_id
//     );

//     let config = load_config("/home/aleph-node/aleph-node-config.toml");

//     for node_url in &config.network.nodes {
//         info!(
//             "Node {} {} NETWORK NODES LOOP NODE URL : {}",
//             node.id,node.ip_address, node_url
//         );
        
//         synchronize_dag_and_epoch(node, client, epoch_id, node_url).await;
//         send_prevote(node, client, node_url, epoch_id, root, proofs, shards).await;
//     }

//     update_epoch_tracker(node, epoch_id).await;
// }

// // Synchronize DAG and next epoch
// // Ensures all nodes have a consistent view of the DAG before moving to the next epoch.
// async fn synchronize_dag_and_epoch(node: &Node, client: &Client, epoch_id: u64, node_url: &String) {
//     let payload = json!({ "epoch_id": epoch_id });
//     if let Err(e) = client
//         .post(format!("http://{}/sync_epoch", node_url))
//         .json(&payload)
//         .send()
//         .await
//     {
//         error!("Node {} {} Failed to synchronize epoch {} with {}. Error: {:?}",  node.id,node.ip_address,epoch_id, node_url, e);
//     } else {
//         info!("Node {} {} Synchronized epoch {} with {}",  node.id,node.ip_address,epoch_id, node_url);
//     }

//     if let Err(e) = ensure_dag_synchronization(node, client, epoch_id, node_url).await {
//         error!("Node {} {} DAG synchronization failed with {} for epoch {}. Error: {:?}",  node.id,node.ip_address,node_url, epoch_id, e);
//     } else {
//         info!("Node {} {} DAG synchronized with {} for epoch {}",  node.id,node.ip_address,node_url, epoch_id);
//     }
// }

// // Send prevote request
// // // Sends a prevote message to all nodes as part of the prevote phase.
// // TODO: Log whether the prevote messages are acknowledged by the recipient nodes for traceability.
// //  TODO: Add mechanisms to handle and retry failed prevote transmissions.
// async fn send_prevote(
//     node: &Node,
//     client: &Client,
//     node_url: &str,
//     epoch_id: u64,
//     root: &Vec<u8>,
//     proofs: &Vec<Vec<Vec<u8>>>,
//     shards: &Vec<Vec<u8>>,
// ) {

// //     pub shards: Vec<Vec<u8>>, // Multiple shards
// // pub proofs: Vec<Vec<Vec<u8>>>,
//     let payload = PrevoteRequest {
//         sender: node.id,
//         root: root.clone(),
//         proofs: proofs.clone(),
//         epoch_id,
//         shards: shards.clone(),
//         node_url: node_url.to_string(),
//     };

//     match client
//         .post(format!("http://{}/prevote", node_url))
//         .json(&payload)
//         .send()
//         .await
//     {
//         Ok(response) if response.status().is_success() => {
//             info!("Node {} {} Prevote sent to {}", node.id,node.ip_address, node_url);
//         }
//         Ok(response) => {
//             error!(
//                 "Node {} {} Failed to send prevote to {}. Status: {}",
//                 node.id,node.ip_address,node_url, response.status()
//             );
//         }
//         Err(e) => {
//             error!("Node {} {} Failed to send prevote to {}. Error: {:?}", node.id,node.ip_address, node_url, e);
//         }
//     }
// }










