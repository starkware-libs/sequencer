use libp2p::Multiaddr;
use validator::Validate;

use crate::NetworkConfig;

#[test]
fn test_network_config_bootstrap_peer_multiaddr_validation() {
    let mut config = NetworkConfig::default();
    config.validate().unwrap();

    let key = [0u8; 32];
    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(key).unwrap();
    let mutliaddr = Multiaddr::empty()
        .with(libp2p::multiaddr::Protocol::Ip4(std::net::Ipv4Addr::LOCALHOST))
        .with(libp2p::multiaddr::Protocol::Tcp(12345))
        .with(libp2p::multiaddr::Protocol::P2p(libp2p::PeerId::from_public_key(&keypair.public())));

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone()]);
    config.validate().unwrap();

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone(), mutliaddr]);
    config.validate().unwrap_err();
}
