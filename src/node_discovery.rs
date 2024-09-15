use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use serde_json::json;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct NodeInfo {
    ip_address: String,
    multiaddr: String,
}

async fn node_discovery(known_nodes: Arc<Mutex<Vec<NodeInfo>>>) {
    let socket = UdpSocket::bind("0.0.0.0:30333").await.expect("Could not bind socket");

    loop {
        let mut buf = [0; 1024];
        let (len, addr) = socket.recv_from(&mut buf).await.expect("Failed to receive data");
        let received_data = String::from_utf8_lossy(&buf[..len]);

        // Parse the incoming message and add it to known nodes
        let node_info: NodeInfo = serde_json::from_str(&received_data).unwrap_or_else(|_| {
            NodeInfo {
                ip_address: addr.to_string(),
                multiaddr: format!("/ip4/{}/tcp/30333", addr),
            }
        });

        let mut known_nodes_locked = known_nodes.lock().unwrap();
        if !known_nodes_locked.iter().any(|node| node.ip_address == node_info.ip_address) {
            known_nodes_locked.push(node_info);
            println!("Discovered new node: {:?}", known_nodes_locked);
        }

        // Periodically write the known nodes to file
        if known_nodes_locked.len() > 0 {
            let known_nodes_json = serde_json::to_string(&*known_nodes_locked).unwrap();
            tokio::fs::write("/home/aleph-node/known_nodes.json", known_nodes_json).await.unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let known_nodes = Arc::new(Mutex::new(Vec::new()));
    
    // Start the node discovery process
    tokio::spawn(node_discovery(known_nodes.clone()));
}
