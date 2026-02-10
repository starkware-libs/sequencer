use apollo_config::secrets::Sensitive;
use validator::Validate;

use crate::test_utils::{DUMMY_MULTIADDR, DUMMY_MULTIADDR2};
use crate::NetworkConfig;

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTIADDR.clone(), DUMMY_MULTIADDR2.clone()]),
        ..NetworkConfig::default()
    };

    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_duplicates_are_invalid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTIADDR.clone(), DUMMY_MULTIADDR.clone()]),
        ..NetworkConfig::default()
    };

    config.validate().unwrap_err();
}

#[test]
fn test_bootstrap_peer_multiaddr_missing_peer_id_is_invalid() {
    let mut config = NetworkConfig::default();
    let mut mutliaddr = DUMMY_MULTIADDR.clone();
    // Remove the trailing /p2p/<peer_id> to test a bootstrap address without peer id.
    mutliaddr.pop();

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
    let mut mutliaddr = DUMMY_MULTIADDR.clone();
    // Remove the trailing /p2p/<peer_id> to test an advertised address without peer id.
    mutliaddr.pop();
    let config =
        NetworkConfig { advertised_multiaddr: Some(mutliaddr), ..NetworkConfig::default() };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_with_peer_id_but_no_secret_key_is_invalid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        secret_key: None,
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_advertised_multiaddr_with_matching_peer_id_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        secret_key: Some(Sensitive::new([0u8; 32].to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_with_non_matching_peer_id_is_invalid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        secret_key: Some(Sensitive::new([1u8; 32].to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_advertised_multiaddr_in_bootstrap_list_both_none_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: None,
        bootstrap_peer_multiaddr: None,
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_in_bootstrap_list_bootstrap_peer_none_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        bootstrap_peer_multiaddr: None,
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_in_bootstrap_list_advertised_none_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: None,
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTIADDR.clone()]),
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_in_bootstrap_list_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTIADDR.clone(), DUMMY_MULTIADDR2.clone()]),
        secret_key: Some(Sensitive::new([0u8; 32].to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_not_in_bootstrap_list_is_invalid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(DUMMY_MULTIADDR.clone()),
        bootstrap_peer_multiaddr: Some(vec![DUMMY_MULTIADDR2.clone()]),
        secret_key: Some(Sensitive::new([0u8; 32].to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}
