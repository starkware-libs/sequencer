<<<<<<< HEAD
use std::net::Ipv4Addr;

use libp2p::identity::Keypair;
use libp2p::PeerId;
||||||| 3f74dd8a6
use libp2p::Multiaddr;
=======
use libp2p::{identity, multiaddr, Multiaddr, PeerId};
>>>>>>> origin/main-v0.14.0
use validator::Validate;

<<<<<<< HEAD
use crate::utils::make_tcp_multiaddr;
||||||| 3f74dd8a6
=======
use crate::test_utils::DUMMY_MULTI_ADDRESS;
>>>>>>> origin/main-v0.14.0
use crate::NetworkConfig;

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

<<<<<<< HEAD
    let key = [0u8; 32];
    let keypair = Keypair::ed25519_from_bytes(key).unwrap();
    let mutliaddr =
        make_tcp_multiaddr(Ipv4Addr::LOCALHOST, 12345, PeerId::from_public_key(&keypair.public()));
||||||| 3f74dd8a6
    let key = [0u8; 32];
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(key).unwrap();
    let mutliaddr = Multiaddr::empty()
        .with(libp2p::multiaddr::Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST))
        .with(libp2p::multiaddr::Protocol::Tcp(12345))
        .with(libp2p::multiaddr::Protocol::P2p(libp2p::PeerId::from_public_key(&keypair.public())));
=======
#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_addr = Multiaddr::empty()
        .with(multiaddr::Protocol::P2p(PeerId::from_public_key(&keypair.public())));

    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTI_ADDRESS.clone(), second_addr]),
        ..NetworkConfig::default()
    };
>>>>>>> origin/main-v0.14.0

    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_duplicates_are_invalid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![
            DUMMY_MULTI_ADDRESS.clone(),
            DUMMY_MULTI_ADDRESS.clone(),
        ]),
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
