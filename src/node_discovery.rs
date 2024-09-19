use std::fs;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use serde_json::json;
use serde::{Serialize, Deserialize};
use aws_sdk_cloudwatchlogs::{Client, Config};
use aws_types::region::Region as AwsRegion;

#[derive(Serialize, Deserialize, Debug)]
struct NodeInfo {
    ip_address: String,
    multiaddr: String,
}

async fn log_to_cloudwatch(client: &Client, log_group_name: &str, log_stream_name: &str, message: &str) {
    // Put log events
    let response = client.put_log_events()
        .log_group_name(log_group_name)
        .log_stream_name(log_stream_name)
        .log_events(message)
        .send()
        .await;

    if let Err(e) = response {
        eprintln!("Failed to log to CloudWatch: {:?}", e);
    }
}

async fn node_discovery(known_nodes: Arc<Mutex<Vec<NodeInfo>>>, client: Client, log_group_name: &str, log_stream_name: &str) {
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

            // Log to CloudWatch
            let log_message = format!("Discovered new node: {:?}", known_nodes_locked);
            log_to_cloudwatch(&client, log_group_name, log_stream_name, &log_message).await;
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

    // Load configuration from a file (e.g., config.json)
    let config: serde_json::Value = serde_json::from_str(&fs::read_to_string("config.json").expect("Unable to read config file"))
        .expect("Failed to parse config file");

    let log_group_name = config["log_group_name"].as_str().expect("log_group_name must be a string");
    let log_stream_name = config["log_stream_name"].as_str().expect("log_stream_name must be a string");
    let log_group_region = config["log_group_region"].as_str().expect("log_group_region must be a string");

    let region = AwsRegion::new(log_group_region);
    let config = Config::builder().region(region).build();
    let client = Client::from_conf(config);
    
    // Start the node discovery process
    tokio::spawn(node_discovery(known_nodes.clone(), client, log_group_name, log_stream_name));
}