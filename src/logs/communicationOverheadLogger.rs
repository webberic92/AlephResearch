use std::fs::OpenOptions;
use std::io::Write;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

pub fn log_communication_overhead(message: &str) {
    let log_file_path = "/home/aleph-node/logs/CommunicationOverhead.log";
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .expect("Unable to open log file");

    let log_entry = format!("[COMMUNICATION_OVERHEAD] {}\n", message);
    file.write_all(log_entry.as_bytes()).expect("Unable to write to log file");

    info!("{}", message);
}