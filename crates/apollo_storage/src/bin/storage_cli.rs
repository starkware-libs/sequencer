use std::net::Ipv4Addr;

use apollo_storage::storage_reader_types::{StorageReaderRequest, StorageReaderResponse};
use clap::Parser;
use reqwest::Client;

#[derive(Parser, Debug)]
#[command(name = "storage_cli", about = "Run storage CLI.")]
struct Args {
    #[arg(long)]
    storage_reader_port: u16,

    /// JSON-encoded request (e.g., '{"Markers":"State"}')
    #[arg(long)]
    request_json: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Parse the JSON string into a StorageReaderRequest
    let request: StorageReaderRequest = serde_json::from_str(&args.request_json)
        .expect("Failed to parse request JSON. Example: '{\"Markers\":\"State\"}'");

    let client = Client::new();
    let url = format!("http://{}:{}/storage/query", Ipv4Addr::LOCALHOST, args.storage_reader_port);

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .expect("Failed to send request to storage reader");

    if !response.status().is_success() {
        let error_text = response.text().await.expect("Failed to read error response");
        eprintln!("Error from storage reader: {}", error_text);
        return;
    }

    let storage_response: StorageReaderResponse =
        response.json().await.expect("Failed to parse response from storage reader");

    println!("Response: {:?}", storage_response);
}
