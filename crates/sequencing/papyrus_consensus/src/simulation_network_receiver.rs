#[cfg(test)]
#[path = "simulation_network_receiver_test.rs"]
mod simulation_network_receiver_test;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::task::Poll;

use futures::{Stream, StreamExt};
use lru::LruCache;
use papyrus_network::network_manager::BroadcastTopicServer;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::ConsensusMessage;
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::BlockHash;
use starknet_api::core::{ContractAddress, PatriciaKey};
use tracing::{debug, instrument};

/// Receiver used to help run simulations of consensus. It has 2 goals in mind:
/// 1. Simulate network failures.
/// 2. Make tests repeatable - This is challenging because simulations involve a noisy environment;
///    so the actual network issues experienced may differ between 2 test runs.
///     - We expect simulations to use fairly reliable networks. That means messages arriving in
///       different order between runs will make up most of the actual noise between runs, as
///       opposed to actual drops or corruption.
///     - Tendermint is, to a large extent, unaffected by minor network reorderings. For instance it
///       doesn't matter if prevotes arrive before or after the Proposal they are for.
///     - This struct is therefore also designed not to be overly sensistive to message order. If
///       message A was dropped by this struct in one run, it should be dropped in the rerun. This
///       is as opposed to using a stateful RNG where the random number is a function of all the
///       previous calls to the RNG.
pub struct NetworkReceiver {
    pub broadcasted_messages_receiver: BroadcastTopicServer<ConsensusMessage>,
    // Cache is used so that repeat sends of a message can be processed differently. For example,
    // if a message is dropped resending it should result in a new decision.
    pub cache: LruCache<ConsensusMessage, u32>,
    pub seed: u64,
    // Probability of dropping a message [0, 1].
    pub drop_probability: f64,
    // Probability of making a message invalid [0, 1].
    pub invalid_probability: f64,
}

impl NetworkReceiver {
    pub fn new(
        broadcasted_messages_receiver: BroadcastTopicServer<ConsensusMessage>,
        cache_size: usize,
        seed: u64,
        drop_probability: f64,
        invalid_probability: f64,
    ) -> Self {
        assert!((0.0..=1.0).contains(&drop_probability));
        assert!((0.0..=1.0).contains(&invalid_probability));
        Self {
            broadcasted_messages_receiver,
            cache: LruCache::new(NonZeroUsize::new(cache_size).unwrap()),
            seed,
            drop_probability,
            invalid_probability,
        }
    }

    /// Determine how to handle a message. If None then the message is silently droppeds. If some,
    /// the returned message is what is sent to the consensus crate.
    ///
    /// Applies `drop_probability` followed by `invalid_probability`. So the probability of an
    /// invalid message is `(1- drop_probability) * invalid_probability`.
    #[instrument(skip(self), level = "debug")]
    pub fn filter_msg(&mut self, msg: ConsensusMessage) -> Option<ConsensusMessage> {
        let msg_hash = self.calculate_msg_hash(&msg);

        if self.should_drop_msg(msg_hash) {
            debug!("Dropping message");
            return None;
        }

        Some(self.maybe_invalidate_msg(msg, msg_hash))
    }

    fn calculate_msg_hash(&mut self, msg: &ConsensusMessage) -> u64 {
        let count = if let Some(count) = self.cache.get_mut(msg) {
            *count += 1;
            *count
        } else {
            self.cache.put(msg.clone(), 1);
            1
        };

        let mut hasher = DefaultHasher::new();
        msg.hash(&mut hasher);
        self.seed.hash(&mut hasher);
        count.hash(&mut hasher);
        hasher.finish()
    }

    fn should_drop_msg(&self, msg_hash: u64) -> bool {
        let prob = (msg_hash as f64) / (u64::MAX as f64);
        prob <= self.drop_probability
    }

    fn maybe_invalidate_msg(
        &mut self,
        mut msg: ConsensusMessage,
        msg_hash: u64,
    ) -> ConsensusMessage {
        if (msg_hash as f64) / (u64::MAX as f64) > self.invalid_probability {
            return msg;
        }
        debug!("Invalidating message");
        // TODO(matan): Allow for invalid votes based on signature.
        match msg {
            ConsensusMessage::Proposal(ref mut proposal) => {
                proposal.block_hash = BlockHash(proposal.block_hash.0 + 1);
            }
            ConsensusMessage::Vote(ref mut vote) => {
                vote.voter = ContractAddress(PatriciaKey::from(msg_hash));
            }
        }
        msg
    }
}

impl Stream for NetworkReceiver {
    type Item = (Result<ConsensusMessage, ProtobufConversionError>, BroadcastedMessageMetadata);

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let item = self.broadcasted_messages_receiver.poll_next_unpin(cx);
            let (msg, broadcasted_message_manager) = match item {
                Poll::Ready(Some((Ok(msg), broadcasted_message_manager))) => {
                    (msg, broadcasted_message_manager)
                }
                _ => return item,
            };
            if let Some(msg) = self.filter_msg(msg) {
                return Poll::Ready(Some((Ok(msg), broadcasted_message_manager)));
            }
        }
    }
}
