use libp2p::Multiaddr;
use once_cell::sync::Lazy;
use validator::Validate;

use crate::NetworkConfig;

static MULTI_ADDR: Lazy<Multiaddr> = Lazy::new(|| {
    let key = [0u8; 32];
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(key).unwrap();
    Multiaddr::empty()
        .with(libp2p::multiaddr::Protocol::P2p(libp2p::PeerId::from_public_key(&keypair.public())))
});

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_accepts_valid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![MULTI_ADDR.clone()]),
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
        .with(libp2p::multiaddr::Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST))
        .with(libp2p::multiaddr::Protocol::Tcp(12345));

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone()]);
    config.validate().unwrap_err();
}
