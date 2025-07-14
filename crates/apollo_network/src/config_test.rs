<<<<<<< HEAD
use std::net::Ipv4Addr;

||||||| 199fa631c
=======
use apollo_network_types::test_utils::DUMMY_MULTI_ADDRESS;
>>>>>>> origin/main-v0.14.0
use libp2p::{identity, multiaddr, Multiaddr, PeerId};
use validator::Validate;

<<<<<<< HEAD
use crate::test_utils::DUMMY_MULTI_ADDRESS;
use crate::utils::make_quic_multiaddr;
||||||| 199fa631c
use crate::test_utils::DUMMY_MULTI_ADDRESS;
=======
>>>>>>> origin/main-v0.14.0
use crate::NetworkConfig;

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_addr =
        make_quic_multiaddr(Ipv4Addr::LOCALHOST, 12345, PeerId::from_public_key(&keypair.public()));

    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTI_ADDRESS.clone(), second_addr]),
        ..NetworkConfig::default()
    };

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
