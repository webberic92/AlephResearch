use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use std::collections::HashSet;
use std::fs;
use serde_json;

async fn discover_nodes(socket: UdpSocket, known_nodes: &HashSet<String>) {
    let mut buf = [0; 1024];
    let mut discovered_nodes = known_nodes.clone();

    loop {
        let (size, addr) = socket.recv_from(&mut buf).await.unwrap();
        let message = String::from_utf8_lossy(&buf[..size]);
        println!("Received message from {}: {}", addr, message);
        
        // Parse and update known nodes
        if let Ok(node) = serde_json::from_str::<String>(&message) {
            if !discovered_nodes.contains(&node) {
                discovered_nodes.insert(node.clone());
                println!("Discovered new node: {}", node);
            }
        }

        // Write discovered nodes to a file
        let nodes_list: Vec<String> = discovered_nodes.iter().cloned().collect();
        let json_data = serde_json::to_string(&nodes_list).unwrap();
        fs::write("/home/aleph-node/known_nodes.json", json_data).unwrap();
        
        // Broadcast the known node list
        let broadcast_message = serde_json::to_string(&nodes_list).unwrap();
        socket.send_to(broadcast_message.as_bytes(), addr).await.unwrap();
    }
}

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
    let known_nodes: HashSet<String> = HashSet::new(); // This could be initialized with known nodes

    println!("Starting node discovery...");
    discover_nodes(socket, &known_nodes).await;
}
