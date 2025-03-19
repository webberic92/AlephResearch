use std::fs::OpenOptions;
use std::io::Write;
use tracing::info;

pub fn log_latency(message: &str) {
    let log_file_path = "/home/aleph-node/logs/Latency.log";
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .expect("Unable to open log file");

    let log_entry = format!("[LATENCY] {}\n", message);
    file.write_all(log_entry.as_bytes()).expect("Unable to write to log file");

    info!("{}", message);
}
