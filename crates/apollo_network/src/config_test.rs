use std::net::Ipv4Addr;

use libp2p::identity::Keypair;
use libp2p::PeerId;
use validator::Validate;

use crate::utils::make_multiaddr;
use crate::NetworkConfig;

#[test]
fn test_network_config_bootstrap_peer_multiaddr_validation() {
    let mut config = NetworkConfig::default();
    config.validate().unwrap();

    let key = [0u8; 32];
    let keypair = Keypair::ed25519_from_bytes(key).unwrap();
    let mutliaddr =
        make_multiaddr(Ipv4Addr::LOCALHOST, 12345, PeerId::from_public_key(&keypair.public()));

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone()]);
    config.validate().unwrap();

    config.bootstrap_peer_multiaddr = Some(vec![mutliaddr.clone(), mutliaddr]);
    config.validate().unwrap_err();
}
