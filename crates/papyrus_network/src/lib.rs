//! This crate is responsible for peer-to-peer messaging.
//!
//! It allows sending and receiving messages between nodes in a peer-to-peer network using
//! user-defined protocols.
//!
//! There are two types of protocol:
//! - **SQMR (Single Query Multiple Response)** Nodes send queries to a specific peer and the peer
//! responds with multiple responses for that query.
//!
//!   The user can send a query to this crate. This crate is responsible for selecting the
//! peer to send the query to.
//!
//!   This crate will forward the responses it gets on a query to the user. The user may report them
//! if they're malformed.
//!
//!   This crate will also send incoming queries to the user, and the user will send back responses
//! for the query. The user may report if the query it received is malformed.
//!
//!   Registering an SQMR protocol is separated into client and server. A node may support only
//!   sending SQMR queries for some protocol or only answering to SQMR queries.
//!
//! - **Broadcast**: Each broadcast protocol is called `Topic`. Nodes can broadcast a message to all
//!   nodes subscribed to a topic. They can also receive broadcasted messages from other nodes.
//!
//! In order to register a protocol, you need to have a type for a message. The type should
//! implement `Into<Vec<u8>>` and `TryFrom<Vec<u8>>`. in SQMR you need two types: one for the
//! query and one for the response.
//!
//!
//! Here's an example for registering an SQMR protocol that sends a number and receives that many
//! random numbers.
//!
//! Client code:
//! ```no_run
//! use futures::channel::{mpsc, oneshot};
//! use futures::{SinkExt, StreamExt};
//! use papyrus_network::{NetworkConfig, NetworkManager};
//!
//! const PROTOCOL: &str = "/my_protocol/1.0.0";
//! const BUFFER_SIZE: usize = 10000;
//!
//! #[derive(Debug)]
//! struct Number(pub usize);
//!
//! impl From<Number> for Vec<u8> {
//!     fn from(num: Number) -> Self {
//!         num.0.to_be_bytes().to_vec()
//!     }
//! }
//!
//! impl TryFrom<Vec<u8>> for Number {
//!     type Error = ();
//!     fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
//!         let bytes_array = bytes.try_into().map_err(|_| ())?;
//!         Ok(Number(usize::from_be_bytes(bytes_array)))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut network_manager = NetworkManager::new(NetworkConfig::default());
//!
//!     let mut query_sender = network_manager
//!         .register_sqmr_protocol_client::<Number, Number>(PROTOCOL.to_string(), BUFFER_SIZE);
//!
//!     'query_loop: for i in 0..10 {
//!         println!("Sending a query for {i} numbers");
//!         let mut responses_manager =
//!             query_sender.send_new_query(Number(i)).await.expect("Failed sending query");
//!
//!         for response_index in 0..i {
//!             let maybe_response_result = responses_manager.next().await;
//!             let Some(Ok(response)) = maybe_response_result else {
//!                 if maybe_response_result.is_none() {
//!                     eprintln!("Got too few responses. Reporting peer.");
//!                 } else {
//!                     eprintln!("Got a bad response. Reporting peer.");
//!                 }
//!                 responses_manager.report_peer();
//!                 // Now in the next queries we'll send, we will receive other peers.
//!                 continue 'query_loop;
//!             };
//!             println!("Received response {response:?}.");
//!         }
//!
//!         if responses_manager.next().await.is_some() {
//!             eprintln!("Got too many responses. Reporting peer.");
//!             responses_manager.report_peer();
//!         }
//!     }
//! }
//! ```
//! Server code:
//! ```no_run
//! use futures::channel::{mpsc, oneshot};
//! use futures::{SinkExt, StreamExt};
//! use papyrus_network::{NetworkConfig, NetworkManager};
//!
//! const PROTOCOL: &str = "/my_protocol/1.0.0";
//! const BUFFER_SIZE: usize = 10000;
//!
//! #[derive(Debug)]
//! struct Number(pub usize);
//!
//! impl From<Number> for Vec<u8> {
//!     fn from(num: Number) -> Self {
//!         num.0.to_be_bytes().to_vec()
//!     }
//! }
//!
//! impl TryFrom<Vec<u8>> for Number {
//!     type Error = ();
//!     fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
//!         let bytes_array = bytes.try_into().map_err(|_| ())?;
//!         Ok(Number(usize::from_be_bytes(bytes_array)))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut network_manager = NetworkManager::new(NetworkConfig::default());
//!
//!     let mut query_receiver = network_manager
//!         .register_sqmr_protocol_server::<Number, Number>(PROTOCOL.to_string(), BUFFER_SIZE);
//!
//!     loop {
//!         let mut query_manager = query_receiver
//!             .next()
//!             .await
//!             .expect("Query receiver from network should never terminate");
//!
//!         let Ok(query_number) = query_manager.query() else {
//!             eprintln!("Peer sent bad query. Reporting it");
//!             query_manager.report_peer();
//!             // Now the next queries we'll receive will be from other peers.
//!             continue;
//!         };
//!         println!("Got a query for {} numbers. Sending responses.", query_number.0);
//!         for i in 0..query_number.0 {
//!             query_manager.send_response(Number(rand::random()));
//!         }
//!     }
//! }
//! ```
//!
//!
//! Here's an example for registering a broadcast protocol:
//! ```no_run
//! #[tokio::main]
//! async fn main() {
//!     // TODO(shahak): Fill this.
//! }
//! ```

mod bin_utils;
mod discovery;
#[cfg(test)]
mod e2e_broadcast_test;
pub mod gossipsub_impl;
mod mixed_behaviour;
pub mod network_manager;
mod peer_manager;
mod sqmr;
#[cfg(test)]
mod test_utils;
mod utils;

use std::collections::BTreeMap;
use std::time::Duration;

use libp2p::Multiaddr;
use papyrus_config::converters::{
    deserialize_optional_vec_u8,
    deserialize_seconds_to_duration,
    serialize_optional_vec_u8,
};
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::validators::validate_vec_u256;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

pub use crate::network_manager::{
    ClientResponsesManager,
    NetworkManager,
    ServerQueryManager,
    SqmrClientSender,
    SqmrServerReceiver,
};

// TODO: add peer manager config to the network config
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
pub struct NetworkConfig {
    pub tcp_port: u16,
    pub quic_port: u16,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,
    pub bootstrap_peer_multiaddr: Option<Multiaddr>,
    #[validate(custom = "validate_vec_u256")]
    #[serde(deserialize_with = "deserialize_optional_vec_u8")]
    pub(crate) secret_key: Option<Vec<u8>>,
    pub chain_id: ChainId,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "tcp_port",
                &self.tcp_port,
                "The port that the node listens on for incoming tcp connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "quic_port",
                &self.quic_port,
                "The port that the node listens on for incoming quic connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "session_timeout",
                &self.session_timeout.as_secs(),
                "Maximal time in seconds that each session can take before failing on timeout.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connection_timeout",
                &self.idle_connection_timeout.as_secs(),
                "Amount of time in seconds that a connection with no active sessions will stay \
                 alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.bootstrap_peer_multiaddr,
            Multiaddr::empty(),
            "bootstrap_peer_multiaddr",
            "The multiaddress of the peer node. It should include the peer's id. For more info: https://docs.libp2p.io/concepts/fundamentals/peers/",
            ParamPrivacyInput::Public,
        ));
        config.extend([ser_param(
            "secret_key",
            &serialize_optional_vec_u8(&self.secret_key),
            "The secret key used for building the peer id. If it's an empty string a random one \
             will be used.",
            ParamPrivacyInput::Private,
        )]);
        config
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            tcp_port: 10000,
            quic_port: 10001,
            session_timeout: Duration::from_secs(120),
            idle_connection_timeout: Duration::from_secs(120),
            bootstrap_peer_multiaddr: None,
            secret_key: None,
            chain_id: ChainId::Mainnet,
        }
    }
}
