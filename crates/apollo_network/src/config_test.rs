use apollo_config::secrets::Sensitive;
use libp2p::{identity, Multiaddr, PeerId};
use validator::Validate;

use crate::test_utils::DUMMY_PEER_ID;
use crate::{MultiaddrConfig, MultiaddrVectorConfig, NetworkConfig};

#[test]
fn test_bootstrap_peer_multiaddr_empty_is_valid() {
    let config = NetworkConfig::default();
    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_unique_addresses_is_valid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_peer_id = PeerId::from_public_key(&keypair.public());

    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(MultiaddrVectorConfig {
            domain: vec!["127.0.0.1".to_string(), "127.0.0.1".to_string()],
            port: vec![10000, 12345],
            peer_id: vec![*DUMMY_PEER_ID, second_peer_id],
        }),
        ..NetworkConfig::default()
    };

    config.validate().unwrap();
}

#[test]
fn test_bootstrap_peer_multiaddr_duplicates_are_invalid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(MultiaddrVectorConfig {
            domain: vec!["127.0.0.1".to_string(), "127.0.0.1".to_string()],
            port: vec![10000, 10000],
            peer_id: vec![*DUMMY_PEER_ID, *DUMMY_PEER_ID],
        }),
        ..NetworkConfig::default()
    };

    config.validate().unwrap_err();
}

#[test]
fn test_bootstrap_peer_multiaddr_mismatched_lengths_are_invalid() {
    let config = NetworkConfig {
        bootstrap_peer_multiaddr: Some(MultiaddrVectorConfig {
            domain: vec!["127.0.0.1".to_string()],
            port: vec![],
            peer_id: vec![*DUMMY_PEER_ID],
        }),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_advertised_multiaddr_none_is_valid() {
    let config = NetworkConfig { advertised_multiaddr: None, ..NetworkConfig::default() };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_without_peer_id_is_valid() {
    let config = NetworkConfig {
        advertised_multiaddr: Some(MultiaddrConfig {
            domain: "127.0.0.1".to_string(),
            port: 12345,
            peer_id: None,
        }),
        ..NetworkConfig::default()
    };
    config.validate().unwrap();
}

#[test]
fn test_advertised_multiaddr_with_peer_id_but_no_secret_key_is_invalid() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let peer_id = PeerId::from_public_key(&keypair.public());
    let config = NetworkConfig {
        advertised_multiaddr: Some(MultiaddrConfig {
            domain: "127.0.0.1".to_string(),
            port: 12345,
            peer_id: Some(peer_id),
        }),
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
    let config = NetworkConfig {
        advertised_multiaddr: Some(MultiaddrConfig {
            domain: "127.0.0.1".to_string(),
            port: 12345,
            peer_id: Some(peer_id),
        }),
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
    let config = NetworkConfig {
        advertised_multiaddr: Some(MultiaddrConfig {
            domain: "127.0.0.1".to_string(),
            port: 12345,
            peer_id: Some(peer_id1),
        }),
        secret_key: Some(Sensitive::new(key2.to_vec())),
        ..NetworkConfig::default()
    };
    config.validate().unwrap_err();
}

#[test]
fn test_multiaddr_config_from_ip_address() {
    let config = MultiaddrConfig {
        domain: "1.2.3.4".to_string(),
        port: 10000,
        peer_id: Some(*DUMMY_PEER_ID),
    };
    let multiaddr: Multiaddr = Multiaddr::try_from(config).unwrap();
    let expected: Multiaddr =
        format!("/ip4/1.2.3.4/tcp/10000/p2p/{}", *DUMMY_PEER_ID).parse().unwrap();
    assert_eq!(multiaddr, expected);
}

#[test]
fn test_multiaddr_config_from_ip6_address() {
    let config =
        MultiaddrConfig { domain: "::1".to_string(), port: 10000, peer_id: Some(*DUMMY_PEER_ID) };
    let multiaddr: Multiaddr = Multiaddr::try_from(config).unwrap();
    let expected: Multiaddr = format!("/ip6/::1/tcp/10000/p2p/{}", *DUMMY_PEER_ID).parse().unwrap();
    assert_eq!(multiaddr, expected);
}

#[test]
fn test_multiaddr_config_from_dns_name() {
    let config = MultiaddrConfig {
        domain: "example.com".to_string(),
        port: 10000,
        peer_id: Some(*DUMMY_PEER_ID),
    };
    let multiaddr: Multiaddr = Multiaddr::try_from(config).unwrap();
    let expected: Multiaddr =
        format!("/dns/example.com/tcp/10000/p2p/{}", *DUMMY_PEER_ID).parse().unwrap();
    assert_eq!(multiaddr, expected);
}

#[test]
fn test_multiaddr_config_without_peer_id() {
    let config = MultiaddrConfig { domain: "1.2.3.4".to_string(), port: 10000, peer_id: None };
    let multiaddr: Multiaddr = Multiaddr::try_from(config).unwrap();
    let expected: Multiaddr = "/ip4/1.2.3.4/tcp/10000".parse().unwrap();
    assert_eq!(multiaddr, expected);
}

#[test]
fn test_multiaddr_vector_config_conversion() {
    let key = [1u8; 32];
    let keypair = identity::Keypair::ed25519_from_bytes(key).unwrap();
    let second_peer_id = PeerId::from_public_key(&keypair.public());

    let config = MultiaddrVectorConfig {
        domain: vec!["1.2.3.4".to_string(), "::1".to_string(), "example.com".to_string()],
        port: vec![10000, 10001, 10002],
        peer_id: vec![*DUMMY_PEER_ID, second_peer_id, *DUMMY_PEER_ID],
    };
    let multiaddrs: Vec<Multiaddr> = config.try_into().unwrap();

    let expected: Vec<Multiaddr> = vec![
        format!("/ip4/1.2.3.4/tcp/10000/p2p/{}", *DUMMY_PEER_ID).parse().unwrap(),
        format!("/ip6/::1/tcp/10001/p2p/{}", second_peer_id).parse().unwrap(),
        format!("/dns/example.com/tcp/10002/p2p/{}", *DUMMY_PEER_ID).parse().unwrap(),
    ];
    assert_eq!(multiaddrs, expected);
}
