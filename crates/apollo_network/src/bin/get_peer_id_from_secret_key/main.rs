use std::env;

use libp2p::identity::Keypair;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <private_key>", args[0]);
        return;
    }

    let raw_str = args[1].clone();
    if !raw_str.starts_with("0x") {
        eprint!("Couldn't deserialize vector. Expected hex string starting with \"0x\"");
        return;
    }

    let hex_str = &raw_str[2..]; // Strip the "0x" prefix

    // Check if the length is a valid size for a private key
    if hex_str.len() != 64 {
        eprintln!("Invalid private key length. Expected 64 characters, got {}", hex_str.len());
        return;
    }

    let mut vector = Vec::new();
    for i in (0..hex_str.len()).step_by(2) {
        let byte_str = &hex_str[i..i + 2];
        let byte = u8::from_str_radix(byte_str, 16).map_err(|e| {
            eprintln!("Couldn't deserialize vector. Failed to parse byte: {byte_str} {e}");
        });
        match byte {
            Ok(b) => vector.push(b),
            Err(_) => return,
        }
    }

    let keypair = Keypair::ed25519_from_bytes(vector).expect("Invalid private key");
    let peer_id = keypair.public().to_peer_id();
    println!("Peer ID: {peer_id}");
}
