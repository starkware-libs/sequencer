# Apollo Network

Apollo Network is a comprehensive peer-to-peer networking crate for Starknet sequencer nodes. It implements the [Starknet P2P specifications](https://github.com/starknet-io/starknet-p2p-specs/) and provides a robust, scalable networking layer built on top of [libp2p](https://libp2p.io/).

## Features

- **SQMR Protocol**: Single Query Multiple Response protocol for efficient peer communication
- **GossipSub Broadcasting**: Reliable message broadcasting across the network
- **Peer Discovery**: Automatic peer discovery using Kademlia DHT and bootstrapping
- **Network Management**: Comprehensive connection and session management
- **Metrics & Monitoring**: Built-in metrics collection and monitoring capabilities
- **Configurable**: Extensive configuration options for various network parameters

## Quick Start

Add Apollo Network to your `Cargo.toml`:

```toml
[dependencies]
apollo_network = { path = "crates/apollo_network" }
starknet_api = "0.7"
futures = "0.3"
tokio = { version = "1.0", features = ["full"] }
```

### Basic Setup

```rust
use apollo_network::{NetworkManager, NetworkConfig};
use apollo_network::network_manager::metrics::NetworkMetrics;
use starknet_api::core::ChainId;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create network configuration
    let config = NetworkConfig {
        port: 10000,
        chain_id: ChainId::Mainnet,
        ..Default::default()
    };

    // Initialize network manager with metrics
    let network_manager = NetworkManager::new(
        config,
        Some("my-starknet-node/1.0.0".to_string()),
        Some(NetworkMetrics::new()),
    );

    // Run the network manager
    network_manager.run().await?;

    Ok(())
}
```

## Protocol Usage

### SQMR (Single Query Multiple Response)

SQMR enables efficient request-response communication where a single query can receive multiple responses.

#### Server Implementation

```rust
use apollo_network::{NetworkManager, NetworkConfig};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct BlockQuery {
    start_height: u64,
    end_height: u64,
}

#[derive(Serialize, Deserialize)]
struct Block {
    height: u64,
    hash: String,
    // ... other fields
}

async fn run_block_server() -> Result<(), Box<dyn std::error::Error>> {
    let mut network_manager = NetworkManager::new(
        NetworkConfig::default(),
        None,
        None,
    );

    // Register as a server for block requests
    let mut server = network_manager.register_sqmr_protocol_server::<BlockQuery, Block>(
        "/starknet/blocks/1.0.0".to_string(),
        100, // buffer size
    );

    tokio::spawn(async move {
        network_manager.run().await
    });

    // Process incoming queries
    while let Some(mut query_manager) = server.next().await {
        match query_manager.query() {
            Ok(query) => {
                // Process query and send responses
                for height in query.start_height..=query.end_height {
                    let block = Block {
                        height,
                        hash: format!("block_hash_{}", height),
                    };
                    if let Err(e) = query_manager.send_response(block).await {
                        eprintln!("Failed to send response: {}", e);
                        break;
                    }
                }
            }
            Err(_) => {
                // Report malicious peer for invalid query
                query_manager.report_peer();
            }
        }
    }

    Ok(())
}
```

#### Client Implementation

```rust
use apollo_network::{NetworkManager, NetworkConfig};
use futures::StreamExt;

async fn run_block_client() -> Result<(), Box<dyn std::error::Error>> {
    let mut network_manager = NetworkManager::new(
        NetworkConfig::default(),
        None,
        None,
    );

    // Register as a client for block requests
    let mut client = network_manager.register_sqmr_protocol_client::<BlockQuery, Block>(
        "/starknet/blocks/1.0.0".to_string(),
        100, // buffer size
    );

    tokio::spawn(async move {
        network_manager.run().await
    });

    // Send a query and process responses
    let query = BlockQuery {
        start_height: 1000,
        end_height: 1010,
    };

    let mut response_manager = client.send_new_query(query).await?;

    while let Some(response_result) = response_manager.next().await {
        match response_result {
            Ok(block) => {
                println!("Received block {}: {}", block.height, block.hash);
            }
            Err(e) => {
                eprintln!("Invalid response: {}", e);
                response_manager.report_peer();
                break;
            }
        }
    }

    Ok(())
}
```

### GossipSub Broadcasting

GossipSub provides efficient message broadcasting with message validation.

```rust
use apollo_network::{NetworkManager, NetworkConfig};
use apollo_network::gossipsub_impl::Topic;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Transaction {
    hash: String,
    from: String,
    to: String,
    amount: u64,
}

async fn run_transaction_broadcast() -> Result<(), Box<dyn std::error::Error>> {
    let mut network_manager = NetworkManager::new(
        NetworkConfig::default(),
        None,
        None,
    );

    // Register for transaction broadcasting
    let topic = Topic::new("transactions");
    let mut channels = network_manager.register_broadcast_topic::<Transaction>(
        topic,
        1000, // buffer size
    )?;

    tokio::spawn(async move {
        network_manager.run().await
    });

    // Spawn a task to handle incoming transactions
    let mut receiver = channels.broadcasted_messages_receiver;
    let mut client = channels.broadcast_topic_client.clone();
    
    tokio::spawn(async move {
        while let Some((result, metadata)) = receiver.next().await {
            match result {
                Ok(transaction) => {
                    if validate_transaction(&transaction) {
                        // Valid transaction - continue propagation
                        let _ = client.continue_propagation(&metadata).await;
                        process_transaction(transaction);
                    } else {
                        // Invalid transaction - report the originator
                        let _ = client.report_peer(metadata).await;
                    }
                }
                Err(e) => {
                    // Malformed message - report the originator
                    eprintln!("Failed to deserialize transaction: {}", e);
                    let _ = client.report_peer(metadata).await;
                }
            }
        }
    });

    // Broadcast transactions
    let transaction = Transaction {
        hash: "tx_hash_123".to_string(),
        from: "alice".to_string(),
        to: "bob".to_string(),
        amount: 100,
    };

    channels.broadcast_topic_client.broadcast_message(transaction).await?;

    Ok(())
}

fn validate_transaction(tx: &Transaction) -> bool {
    // Implement transaction validation logic
    !tx.hash.is_empty() && tx.amount > 0
}

fn process_transaction(tx: Transaction) {
    println!("Processing transaction: {}", tx.hash);
    // Implement transaction processing logic
}
```

## Configuration

The networking layer can be extensively configured through the `NetworkConfig` struct:

```rust
use apollo_network::{NetworkConfig, discovery::DiscoveryConfig, peer_manager::PeerManagerConfig};
use starknet_api::core::ChainId;
use std::time::Duration;
use libp2p::Multiaddr;

let config = NetworkConfig {
    // Basic network settings
    port: 10000,
    chain_id: ChainId::Mainnet,
    
    // Timeout configuration
    session_timeout: Duration::from_secs(120),
    idle_connection_timeout: Duration::from_secs(120),
    
    // Bootstrap peers for initial connectivity
    bootstrap_peer_multiaddr: Some(vec![
        "/ip4/1.2.3.4/tcp/10000/p2p/12D3KooWQYHvEJzuBP...".parse()?,
        "/ip4/5.6.7.8/tcp/10000/p2p/12D3KooWDifferentPeer...".parse()?,
    ]),
    
    // Optional: Use a deterministic peer ID
    secret_key: Some(your_32_byte_ed25519_key),
    
    // Optional: Advertise a specific external address
    advertised_multiaddr: Some("/ip4/203.0.113.1/tcp/10000".parse()?),
    
    // Discovery and peer management configuration
    discovery_config: DiscoveryConfig::default(),
    peer_manager_config: PeerManagerConfig::default(),
    
    // Buffer sizes for message handling
    broadcasted_message_metadata_buffer_size: 100000,
    reported_peer_ids_buffer_size: 100000,
};
```

## Architecture

The crate is organized into several key modules:

- **`network_manager`**: Core networking functionality and main entry point
- **`sqmr`**: Single Query Multiple Response protocol implementation
- **`gossipsub_impl`**: GossipSub-based message broadcasting
- **`discovery`**: Peer discovery mechanisms (Kademlia DHT, bootstrapping)
- **`peer_manager`**: Peer lifecycle and reputation management
- **`misconduct_score`**: Peer reputation scoring system

## Error Handling

The crate provides comprehensive error handling:

```rust
use apollo_network::{NetworkError, NetworkManager, NetworkConfig};

async fn handle_network_errors() {
    let network_manager = NetworkManager::new(
        NetworkConfig::default(),
        None,
        None,
    );

    match network_manager.run().await {
        Ok(_) => {
            // Network manager completed (should not happen in normal operation)
        }
        Err(NetworkError::DialError(e)) => {
            eprintln!("Failed to dial peer: {}", e);
        }
        Err(NetworkError::BroadcastChannelsDropped { topic_hash }) => {
            eprintln!("Broadcast channels dropped for topic: {:?}", topic_hash);
        }
    }
}
```

## Metrics and Monitoring

The crate includes built-in metrics support:

```rust
use apollo_network::network_manager::metrics::NetworkMetrics;

// Initialize metrics
let metrics = NetworkMetrics::new();

// Use with network manager
let network_manager = NetworkManager::new(
    config,
    Some("node-version".to_string()),
    Some(metrics),
);
```

## Testing

The crate includes comprehensive test utilities. For testing, topics use identity-based hashing for deterministic behavior:

```rust
#[cfg(test)]
mod tests {
    use apollo_network::gossipsub_impl::Topic;
    
    #[test]
    fn test_topic_creation() {
        let topic = Topic::new("test-topic");
        // In test mode, topics use identity hashing for predictability
        assert_eq!(topic.hash().as_str(), "test-topic");
    }
}
```

## Contributing

When contributing to Apollo Network:

1. Ensure all public APIs are properly documented
2. Add examples for new functionality
3. Include comprehensive tests
4. Update this README for significant changes
5. Follow the existing code style and patterns

## License

This project is licensed under the same terms as the Apollo project.
