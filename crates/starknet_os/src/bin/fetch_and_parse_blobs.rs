use c_kzg::BYTES_PER_BLOB;
use clap::Parser;
use hex;
use starknet_types_core::felt::Felt;

use starknet_os::hints::hint_implementation::kzg::utils::decode_blobs;
use starknet_os::hints::hint_implementation::state_diff_encryption::utils::{
    decrypt_state_diff_from_blobs, DecryptionError,
};
use starknet_os::io::os_output_types::{PartialOsStateDiff, TryFromOutputIter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to blob file (hex encoded, one blob per line, or binary file)
    #[arg(short, long)]
    blob_file: String,

    /// Optional private key for decryption (hex format)
    #[arg(long)]
    private_key: Option<String>,

    /// Committee index for decryption (default: 0)
    #[arg(long, default_value = "0")]
    committee_index: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Read blobs from file
    let blob_data = read_blobs_from_file(&args.blob_file)?;

    let private_key = args.private_key.as_ref().map(|key_str| {
        Felt::from_hex(key_str)
            .expect("Failed to parse private key")
    });

    parse_blobs_to_state_diff(blob_data, private_key, args.committee_index)
}

fn read_blobs_from_file(file_path: &str) -> Result<Vec<[u8; BYTES_PER_BLOB]>, Box<dyn std::error::Error>> {
    use std::fs;

    println!("Reading blobs from file: {}", file_path);
    
    let content = fs::read(file_path)?;
    
    // Try to parse as hex strings (one per line)
    let lines: Vec<&str> = std::str::from_utf8(&content)?
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if !lines.is_empty() {
        // Try parsing as hex strings
        let mut blob_data = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let line = line.strip_prefix("0x").unwrap_or(line);
            let blob_bytes = hex::decode(line)
                .map_err(|e| format!("Failed to decode blob {} from hex: {}", i, e))?;
            
            if blob_bytes.len() != BYTES_PER_BLOB {
                return Err(format!(
                    "Blob {} has wrong size: expected {}, got {}",
                    i, BYTES_PER_BLOB, blob_bytes.len()
                ).into());
            }

            let blob_array: [u8; BYTES_PER_BLOB] = blob_bytes
                .try_into()
                .map_err(|_| format!("Failed to convert blob {} to array", i))?;
            
            blob_data.push(blob_array);
        }
        
        if !blob_data.is_empty() {
            println!("Read {} blobs from file", blob_data.len());
            return Ok(blob_data);
        }
    }

    // Try parsing as raw binary (must be multiple of BYTES_PER_BLOB)
    if content.len() % BYTES_PER_BLOB == 0 {
        let num_blobs = content.len() / BYTES_PER_BLOB;
        let mut blob_data = Vec::new();
        for i in 0..num_blobs {
            let start = i * BYTES_PER_BLOB;
            let end = start + BYTES_PER_BLOB;
            let blob_array: [u8; BYTES_PER_BLOB] = content[start..end]
                .try_into()
                .map_err(|_| format!("Failed to extract blob {}", i))?;
            blob_data.push(blob_array);
        }
        println!("Read {} blobs from binary file", blob_data.len());
        return Ok(blob_data);
    }

    Err(format!(
        "File size {} is not a multiple of blob size {}",
        content.len(), BYTES_PER_BLOB
    ).into())
}

/// Helper function to parse blobs once we have the raw blob data
fn parse_blobs_to_state_diff(
    blob_data: Vec<[u8; BYTES_PER_BLOB]>,
    private_key: Option<Felt>,
    committee_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Decoding {} blobs...", blob_data.len());

    // If we have a private key, try decryption first
    if let Some(private_key) = private_key {
        println!("\n=== Attempting decryption with private key ===");
        match decrypt_state_diff_from_blobs(blob_data.clone(), private_key, committee_index) {
            Ok(state_diff) => {
                println!("✓ Successfully decrypted and parsed as PartialOsStateDiff!");
                println!("  Contracts: {}", state_diff.contracts.len());
                println!("  Classes: {}", state_diff.classes.len());
                println!("The PartialOsStateDiff is: {:#?}", state_diff);
                return Ok(());
            }
            Err(DecryptionError::Fft(e)) => {
                println!("✗ FFT error during decryption: {}", e);
                println!("  Trying without decryption...");
            }
            Err(DecryptionError::Parsing(e)) => {
                println!("✗ Parsing error after decryption: {}", e);
                println!("  Trying without decryption...");
            }
        }
    }

    // Decode blobs without decryption
    let decoded_felts = decode_blobs(blob_data)?;
    println!("Decoded {} field elements", decoded_felts.len());

    // Try parsing as PartialOsStateDiff (with decompression)
    println!("\n=== Attempting to parse as PartialOsStateDiff ===");
    let mut iter = decoded_felts.iter().copied();
    match PartialOsStateDiff::try_from_output_iter(&mut iter, None) {
        Ok(state_diff) => {
            println!("✓ Successfully parsed as PartialOsStateDiff!");
            println!("  Contracts: {}", state_diff.contracts.len());
            println!("  Classes: {}", state_diff.classes.len());
            println!("The PartialOsStateDiff is: {:#?}", state_diff);
            Ok(())
        }
        Err(e) => {
            println!("✗ Failed to parse as PartialOsStateDiff: {}", e);
            Err(format!("Failed to parse blobs as PartialOsStateDiff: {}", e).into())
        }
    }
}

