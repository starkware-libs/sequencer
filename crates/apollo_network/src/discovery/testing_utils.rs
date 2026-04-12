use libp2p::swarm::ConnectionId;

/// Increments a ConnectionId by 1.
///
/// This is a utility for tests that need to simulate different connection IDs
/// (e.g., when testing scenarios where an external entity establishes a separate connection).
pub(crate) fn next_connection_id(conn_id: ConnectionId) -> ConnectionId {
    ConnectionId::new_unchecked(format!("{conn_id}").parse::<usize>().unwrap() + 1)
}
