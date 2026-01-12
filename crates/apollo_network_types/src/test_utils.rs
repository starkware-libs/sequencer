use apollo_test_utils::{auto_impl_get_test_instance, GetTestInstance};
use lazy_static::lazy_static;
use libp2p::{identity, multiaddr, Multiaddr, PeerId};
use rand_chacha::ChaCha8Rng;

use crate::network_types::{BroadcastedMessageMetadata, OpaquePeerId};

lazy_static! {
    pub static ref DUMMY_PEER_ID: PeerId = {
        let keypair = get_keypair(0);
        PeerId::from_public_key(&keypair.public())
    };
    pub static ref DUMMY_MULTI_ADDRESS: Multiaddr =
        Multiaddr::empty().with(multiaddr::Protocol::P2p(*DUMMY_PEER_ID));
    pub static ref DUMMY_KEYPAIR: identity::Keypair = get_keypair(0);
}

/// Returns a `PeerId`` used to testing. Different indices will yield different `PeerId`s.
pub fn get_peer_id(index: u8) -> PeerId {
    PeerId::from_public_key(&get_keypair(index).public())
}

impl GetTestInstance for OpaquePeerId {
    // TODO(Shahak): use the given rng by copying the libp2p implementation.
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::private_new(*DUMMY_PEER_ID)
    }
}

auto_impl_get_test_instance! {
    pub struct BroadcastedMessageMetadata {
        pub originator_id: OpaquePeerId,
        pub encoded_message_length: usize,
    }
}

pub fn get_keypair(i: u8) -> identity::Keypair {
    let key = [i; 32];
    identity::Keypair::ed25519_from_bytes(key).unwrap()
}
