//! Single Query Multiple Response (SQMR) protocol implementation.
//!
//! SQMR is a custom protocol that enables efficient request-response communication
//! patterns where a single query can receive multiple responses from peers. This is
//! particularly useful for data synchronization scenarios like block or transaction
//! propagation.
//!
//! ## Protocol Overview
//!
//! The SQMR protocol operates as follows:
//!
//! 1. **Query Phase**: A client sends a single query to a server peer
//! 2. **Response Phase**: The server can send multiple responses back
//! 3. **Completion**: The server indicates completion by closing the stream
//! 4. **Timeout**: Sessions have configurable timeouts to prevent hanging
//!
//! ## Key Features
//!
//! - **Multiple Responses**: Unlike traditional request-response, supports multiple responses per
//!   query
//! - **Session Management**: Proper session lifecycle with unique identifiers
//! - **Error Handling**: Comprehensive error reporting and session failure handling
//! - **Configurable Timeouts**: Prevents hanging sessions with configurable timeouts
//! - **Peer Reporting**: Built-in support for reporting malicious peer behavior
//!
//! ## Components
//!
//! - [`behaviour`]: libp2p behavior implementation for SQMR protocol
//! - [`handler`]: Connection handler for managing individual sessions
//! - [`protocol`]: Protocol definition and message framing
//! - Session types: [`InboundSessionId`], [`OutboundSessionId`], [`SessionId`]
//! - Configuration: [`Config`] for protocol parameters
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! # use apollo_network::sqmr::Config;
//! # use std::time::Duration;
//! // Configure SQMR protocol
//! let config = Config { session_timeout: Duration::from_secs(120) };
//!
//! // Sessions are managed automatically by the NetworkManager
//! // when registering SQMR protocol clients and servers
//! ```

pub mod behaviour;
pub mod handler;
mod messages;
pub mod protocol;

#[cfg(test)]
mod flow_test;

use std::time::Duration;

pub use behaviour::{Behaviour, ToOtherBehaviourEvent};
use libp2p::{PeerId, StreamProtocol};

use crate::Bytes;

/// Unique identifier for outbound SQMR sessions.
///
/// An outbound session is initiated by this node when sending a query to another peer.
/// Each outbound session has a unique ID that tracks the query-response lifecycle.
///
/// # Usage
///
/// Outbound session IDs are automatically generated when sending queries through
/// the SQMR client interface. They're used internally to match incoming responses
/// with their originating queries.
#[derive(Clone, Copy, Debug, Default, derive_more::Display, Eq, Hash, PartialEq)]
pub struct OutboundSessionId {
    /// The numeric session identifier.
    pub value: usize,
}

/// Unique identifier for inbound SQMR sessions.
///
/// An inbound session is created when this node receives a query from another peer.
/// Each inbound session has a unique ID that tracks the query processing and
/// response sending lifecycle.
///
/// # Usage
///
/// Inbound session IDs are automatically generated when queries are received
/// through the SQMR server interface. They're used to manage the session
/// state and ensure responses are sent on the correct session.
#[derive(
    Clone, Copy, Debug, Default, derive_more::Display, Eq, Hash, PartialEq, PartialOrd, Ord,
)]
pub struct InboundSessionId {
    /// The numeric session identifier.
    pub value: usize,
}

/// Unified session identifier that can represent either inbound or outbound sessions.
///
/// This enum is used in contexts where the session direction (inbound vs outbound)
/// doesn't matter, such as session cleanup, error reporting, or metrics collection.
///
/// # Examples
///
/// ```rust
/// use apollo_network::sqmr::{InboundSessionId, OutboundSessionId, SessionId};
///
/// let inbound = SessionId::from(InboundSessionId { value: 1 });
/// let outbound = SessionId::from(OutboundSessionId { value: 2 });
///
/// // Both can be handled uniformly in cleanup code
/// match inbound {
///     SessionId::InboundSessionId(id) => println!("Cleaning up inbound session {}", id),
///     SessionId::OutboundSessionId(id) => println!("Cleaning up outbound session {}", id),
/// }
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionId {
    /// An outbound session identifier.
    OutboundSessionId(OutboundSessionId),
    /// An inbound session identifier.
    InboundSessionId(InboundSessionId),
}

impl From<OutboundSessionId> for SessionId {
    fn from(outbound_session_id: OutboundSessionId) -> Self {
        Self::OutboundSessionId(outbound_session_id)
    }
}

impl From<InboundSessionId> for SessionId {
    fn from(inbound_session_id: InboundSessionId) -> Self {
        Self::InboundSessionId(inbound_session_id)
    }
}

#[derive(Debug)]
pub enum GenericEvent<SessionError> {
    NewInboundSession {
        query: Bytes,
        inbound_session_id: InboundSessionId,
        peer_id: PeerId,
        protocol_name: StreamProtocol,
    },
    ReceivedResponse {
        outbound_session_id: OutboundSessionId,
        response: Bytes,
        peer_id: PeerId,
    },
    SessionFailed {
        session_id: SessionId,
        error: SessionError,
    },
    SessionFinishedSuccessfully {
        session_id: SessionId,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Config {
    pub session_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self { session_timeout: Duration::from_secs(10) }
    }
}
