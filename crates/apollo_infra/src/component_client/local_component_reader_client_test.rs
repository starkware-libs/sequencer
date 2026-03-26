use tokio::sync::watch;

use super::LocalComponentReaderClient;
use crate::component_definitions::ComponentReaderClient;

#[test]
fn get_value_returns_initial_value() {
    let (_, value_rx) = watch::channel(42u32);
    let client = LocalComponentReaderClient::new(value_rx);
    assert_eq!(client.get_value(), 42);
}

#[test]
fn get_value_reflects_updated_value() {
    let (value_tx, value_rx) = watch::channel(1u32);
    let client = LocalComponentReaderClient::new(value_rx);
    value_tx.send(99).unwrap();
    assert_eq!(client.get_value(), 99);
}

#[test]
fn cloned_client_shares_same_channel() {
    let (value_tx, value_rx) = watch::channel(0u32);
    let client_a = LocalComponentReaderClient::new(value_rx);
    let client_b = client_a.clone();
    value_tx.send(7).unwrap();
    assert_eq!(client_a.get_value(), client_b.get_value());
}
