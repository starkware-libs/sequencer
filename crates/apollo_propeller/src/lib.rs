//! # Propeller Protocol Implementation
//!
//! Implementation of a simplified block propagation protocol for libp2p, inspired by Solana's
//! Turbine.
//!
//! Propeller is a tree-structured block dissemination protocol designed to minimize
//! publisher egress bandwidth while ensuring rapid and resilient block propagation
//! across a high-throughput network.
//!
//! ## Inspiration and Key Differences from Turbine
//!
//! This implementation is inspired by Solana's Turbine protocol but differs in several key ways:
//!
//! 1. **Fewer, Larger Shards**: Propeller uses fewer shards that are larger in size compared to
//!    Turbine's many small shards, reducing overhead and simplifying the protocol.
//!
//! 2. **Standard Connections**: Uses normal libp2p stream connections instead of UDP/QUIC
//!    datagrams, providing better reliability and easier integration with existing libp2p
//!    infrastructure.
//!
//! ## Key Features
//!
//! - **Dynamic Tree Topology**: Per-shard deterministic tree generation
//! - **Weight-Based Selection**: Higher weight nodes positioned closer to root
//! - **Reed-Solomon Erasure Coding**: Self-healing network with configurable FEC ratios
//! - **Attack Resistance**: Dynamic trees prevent targeted attacks
//!
//! ## Usage
//!
//! ```rust
//! use libp2p_identity::{Keypair, PeerId};
//! use libp2p_propeller::{Behaviour, Config, MessageAuthenticity};
//!
//! // Create propeller behaviour with custom config
//! let config = Config::builder()
//!     .fec_data_shards(16) // 16 data shards
//!     .fec_coding_shards(16) // 16 coding shards
//!     .fanout(100) // Fanout of 100
//!     .build();
//!
//! // Generate keypairs for valid peer IDs with extractable public keys
//! let local_keypair = Keypair::generate_ed25519();
//! let local_peer_id = PeerId::from(local_keypair.public());
//! let mut propeller = Behaviour::new(MessageAuthenticity::Author(local_peer_id), config.clone());
//!
//! // Add peers with weights (including local peer required by tree manager)
//! let peer1_keypair = Keypair::generate_ed25519();
//! let peer1 = PeerId::from(peer1_keypair.public());
//! let peer2_keypair = Keypair::generate_ed25519();
//! let peer2 = PeerId::from(peer2_keypair.public());
//! propeller.set_peers(vec![(local_peer_id, 2000), (peer1, 1000), (peer2, 500)]).unwrap();
//!
//! // Broadcast data (publisher sends to tree root, then propagates through tree)
//! // Data size must be divisible by number of data shards
//! let data_to_broadcast = vec![42u8; 1024]; // Example: 1024 bytes, divisible by 16 shards
//! propeller.broadcast(data_to_broadcast, 0).unwrap();
//! ```

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

mod behaviour;
mod config;
mod generated;
mod handler;
mod merkle;
mod message;
mod metrics;
mod protocol;
pub mod reed_solomon;
mod signature;
mod tree;
mod types;

pub use self::behaviour::{Behaviour, MessageAuthenticity};
pub use self::config::{Config, ConfigBuilder, ValidationMode};
pub use self::handler::{Handler, HandlerIn, HandlerOut};
pub use self::merkle::{MerkleHash, MerkleProof, MerkleTree};
pub use self::message::PropellerMessage;
pub use self::metrics::PropellerMetrics;
pub use self::types::{
    Event,
    MessageRoot,
    PeerSetError,
    ReconstructionError,
    ShardIndex,
    ShardPublishError,
    ShardValidationError,
    TreeGenerationError,
};
