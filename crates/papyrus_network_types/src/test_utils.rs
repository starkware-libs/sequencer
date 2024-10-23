use libp2p::PeerId;
use papyrus_test_utils::{auto_impl_get_test_instance, GetTestInstance};
use rand_chacha::ChaCha8Rng;

use crate::network_types::{BroadcastedMessageMetadata, OpaquePeerId};

impl GetTestInstance for OpaquePeerId {
    // TODO: use the given rng by copying the libp2p implementation.
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::private_new(PeerId::random())
    }
}

auto_impl_get_test_instance! {
    pub struct BroadcastedMessageMetadata {
        pub originator_id: OpaquePeerId,
    }
}
