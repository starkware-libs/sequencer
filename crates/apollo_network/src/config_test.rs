use std::net::Ipv4Addr;

use apollo_config::secrets::Sensitive;
use libp2p::{PeerId, identity};
use validator::Validate;

use crate::NetworkConfig;
use crate::test_utils::DUMMY_MULTI_ADDRESS;
use crate::utils::make_multiaddr;

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_addr = make_multiaddr(
        Ipv4Addr::LOCALHOST,
        12345,
        Some(PeerId::from_public_key(&keypair.public())),
    );

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
    let mutliaddr = make_multiaddr(Ipv4Addr::LOCALHOST, 12345, None);

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone()]);
    config.validate().unwrap_err();
}

#[test]
fn test_advertised_multiaddr_none_is_valid() {
    let config = NetworkConfig { advertised_multiaddr: None, ..NetworkConfig::default() };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_without_peer_id_is_valid() {
    let mutliaddr = make_multiaddr(Ipv4Addr::LOCALHOST, 12345, None);
    let config =
        NetworkConfig { advertised_multiaddr: Some(mutliaddr), ..NetworkConfig::default() };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_with_peer_id_but_no_secret_key_is_invalid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let peer_id = PeerId::from_public_key(&keypair.public());
    let mutliaddr = make_multiaddr(Ipv4Addr::LOCALHOST, 12345, Some(peer_id));
    let config = NetworkConfig {
        advertised_multiaddr: Some(mutliaddr),
        secret_key: None,
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_advertised_multiaddr_with_matching_peer_id_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let peer_id = PeerId::from_public_key(&keypair.public());
    let mutliaddr = make_multiaddr(Ipv4Addr::LOCALHOST, 12345, Some(peer_id));
    let config = NetworkConfig {
        advertised_multiaddr: Some(mutliaddr),
        secret_key: Some(Sensitive::new(key.to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_with_non_matching_peer_id_is_invalid() {
    let key1 = [1u8; 32];
    let key2 = [2u8; 32];
    let keypair1 = identity::Keypair::ed25519_from_bytes(key1).unwrap();
    let peer_id1 = PeerId::from_public_key(&keypair1.public());
    let mutliaddr = make_multiaddr(Ipv4Addr::LOCALHOST, 12345, Some(peer_id1));
    let config = NetworkConfig {
        advertised_multiaddr: Some(mutliaddr),
        secret_key: Some(Sensitive::new(key2.to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}
