use std::net::Ipv4Addr;

use libp2p::{identity, PeerId};
use validator::Validate;

use crate::test_utils::DUMMY_MULTI_ADDRESS;
use crate::utils::make_multiaddr;
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
fn test_secret_key_advertised_multiaddr_match_is_valid() {
    let secret_key = vec![1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(secret_key.clone()).unwrap();
    let advertised_multiaddr =
        make_multiaddr(Ipv4Addr::LOCALHOST, 12345, Some(keypair.public().to_peer_id()));

    let config = NetworkConfig {
        secret_key: Some(secret_key.into()),
        advertised_multiaddr: Some(advertised_multiaddr),
        ..NetworkConfig::default()
    };

    config.validate().unwrap();
}

#[test]
fn test_secret_key_advertised_multiaddr_mismatch_is_invalid() {
    let secret_key = vec![1u8; 32];

    let config = NetworkConfig {
        secret_key: Some(secret_key.into()),
        advertised_multiaddr: Some(DUMMY_MULTI_ADDRESS.clone()),
        ..NetworkConfig::default()
    };

    config.validate().unwrap_err();
}

#[test]
fn test_secret_key_advertised_multiaddr_missing_is_valid() {
    let config =
        NetworkConfig { secret_key: None, advertised_multiaddr: None, ..NetworkConfig::default() };

    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_secret_key_missing_is_invalid() {
    let config = NetworkConfig {
        secret_key: None,
        advertised_multiaddr: Some(DUMMY_MULTI_ADDRESS.clone()),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_secret_key_advertised_multiaddr_missing_is_invalid() {
    let config = NetworkConfig {
        secret_key: Some(vec![1u8; 32].into()),
        advertised_multiaddr: None,
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}
