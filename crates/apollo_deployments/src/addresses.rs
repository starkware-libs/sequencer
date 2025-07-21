// Derived using `get_peer_id_from_secret_key` binary, where the secret key of node with index `id`
// is format!("0x010101010101010101010101010101010101010101010101010101010101010{}", id + 1)

const PEER_IDS: [&str; 6] = [
    "12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
    "12D3KooWCPzcTZ4ymgyveYaFfZ4bfWsBEh2KxuxM3Rmy7MunqHwe",
    "12D3KooWT3eoCYeMPrSNnF1eQHimWFDiqPkna7FUD6XKBw8oPiMp",
    "12D3KooWFdTjV6DXVJfQFisTXadCsqGzCbEnJJWzc6mXSPwy9g54",
    "12D3KooWJMukrrip9sUyto28eiofqxyXiw9sfTJuZeQfHUujWPX8",
    "12D3KooWMqkzSDGNQg9WDDPdu7nQgAPpqTY3YqZ2XUYqJzmUhmVu",
];

pub(crate) fn get_peer_id(node_id: usize) -> String {
    assert!(node_id < PEER_IDS.len(), "Node index out of bounds: {node_id}");
    PEER_IDS[node_id].to_string()
}

pub(crate) fn get_p2p_address(dns: &str, port: u16, peer_id: &str) -> String {
    format!("/dns/{dns}/udp/{port}/quic-v1/p2p/{peer_id}")
}
