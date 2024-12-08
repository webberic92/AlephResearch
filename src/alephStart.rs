use reqwest::Client;
use serde_json::json;
use tracing::info;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let client = Client::new();

    let nodes = vec!["http://127.0.0.1:30333"];

    // Simulate sending proposals
    for node in &nodes {
        let response = client
            .post(format!("{}/propose", node))
            .json(&json!({
                "sender": 1,
                "shard": vec![1, 2, 3],
                "proof": vec![4, 5, 6],
                "root": vec![7, 8, 9]
            }))
            .send()
            .await?;

        info!("Response from {}: {:?}", node, response.text().await?);
    }

    // Simulate sending prevotes
    for node in &nodes {
        let response = client
            .post(format!("{}/prevote", node))
            .json(&json!({
                "sender": 1,
                "root": vec![7, 8, 9]
            }))
            .send()
            .await?;

        info!("Response from {}: {:?}", node, response.text().await?);
    }

    // Simulate sending commits
    for node in &nodes {
        let response = client
            .post(format!("{}/commit", node))
            .json(&json!({
                "sender": 1,
                "root": vec![7, 8, 9]
            }))
            .send()
            .await?;

        info!("Response from {}: {:?}", node, response.text().await?);
    }

    Ok(())
}
