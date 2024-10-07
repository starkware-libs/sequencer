use std::net::SocketAddr;

use tokio::net::TcpListener;

// TODO(Nadin): Merge this get_available_socket function with the one in the test_integration crate
// and remove the duplicate there.
/// Returns a unique IP address and port for testing purposes.
/// Tests run in parallel, so servers (like RPC or web) running on separate tests must have
/// different ports, otherwise the server will fail with "address already in use".
pub async fn get_available_socket() -> SocketAddr {
    // Dynamically select port.
    // First, set the port to 0 (dynamic port).
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to address")
        // Then, resolve to the actual selected port.
        .local_addr()
        .expect("Failed to get local address")
}
