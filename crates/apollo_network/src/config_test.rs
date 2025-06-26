use lazy_static::lazy_static;
use libp2p::{identity, multiaddr, Multiaddr, PeerId};
use validator::Validate;

use crate::NetworkConfig;

lazy_static! {
    static ref MULTI_ADDR: Multiaddr = {
        let key = [0u8; 32];
        let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
        Multiaddr::empty()
            .with(multiaddr::Protocol::P2p(PeerId::from_public_key(&keypair.public())))
    };
}

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_addr = Multiaddr::empty()
        .with(multiaddr::Protocol::P2p(PeerId::from_public_key(&keypair.public())));

    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![MULTI_ADDR.clone(), second_addr]),
        ..NetworkConfig::default()
    };

    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_duplicates_are_invalid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![MULTI_ADDR.clone(), MULTI_ADDR.clone()]),
        ..NetworkConfig::default()
    };

    config.validate().unwrap_err();
}

#[test]
fn test_bootstrap_peer_multiaddr_missing_peer_id_is_invalid() {
    let mut config = NetworkConfig::default();

    let mutliaddr = Multiaddr::empty()
        .with(multiaddr::Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST))
        .with(multiaddr::Protocol::Tcp(12345));

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone()]);
    config.validate().unwrap_err();
}
