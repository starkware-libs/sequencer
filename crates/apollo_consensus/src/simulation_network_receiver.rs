#[cfg(test)]
#[path = "simulation_network_receiver_test.rs"]
mod simulation_network_receiver_test;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::task::Poll;

use starknet_api::core::{ContractAddress, PatriciaKey};
use apollo_network::network_manager::BroadcastTopicServer;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::Vote;
use apollo_protobuf::converters::ProtobufConversionError;
use futures::{Stream, StreamExt};
use lru::LruCache;
use tracing::{debug, instrument};

/// Receiver which can simulate network issues in a repeatable manner. Simulates drops and network
/// corruption. The errors are meant to be repeatable regardless of the order of messages received.
///
/// Being indifferent to the order of messages on the network means that we don't have a state which
/// changes across all messages. If we were truly stateless though we would treat resends of
/// messages all the same, meaning that a dropped message would always be dropped. To avoid this we
/// have the cache, which allows us to treat resends of a specific message differently.
pub struct NetworkReceiver {
    pub broadcasted_messages_receiver: BroadcastTopicServer<Vote>,
    // Cache is used so that repeat sends of a message can be processed differently. For example,
    // if a message is dropped resending it should result in a new decision.
    pub cache: LruCache<Vote, u32>,
    pub seed: u64,
    // Probability of dropping a message [0, 1].
    pub drop_probability: f64,
    // Probability of making a message invalid [0, 1].
    pub invalid_probability: f64,
}

impl NetworkReceiver {
    /// Creates a new NetworkReceiver.
    ///
    /// Inputs:
    /// - `broadcasted_messages_receiver`: The receiver to listen to.
    /// - `cache_size`: Determines the size of the cache. A small cache risks acting the same across
    ///   resends of a given message.    /// - `seed`: Seed for the random number generator.
    /// - `drop_probability`: Probability of dropping a message [0, 1].
    /// - `invalid_probability`: Probability of making a message invalid [0, 1].
    pub fn new(
        broadcasted_messages_receiver: BroadcastTopicServer<Vote>,
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

    /// Determine how to handle a message. If None then the message is silently dropped. If some,
    /// the returned message is what is sent to the consensus crate.
    ///
    /// Applies `drop_probability` followed by `invalid_probability`. So the probability of an
    /// invalid message is `(1- drop_probability) * invalid_probability`.
    #[instrument(skip(self), level = "debug")]
    pub fn filter_msg(&mut self, msg: Vote) -> Option<Vote> {
        let msg_hash = self.calculate_msg_hash(&msg);

        if self.should_drop_msg(msg_hash) {
            debug!("Dropping message");
            return None;
        }

        Some(self.maybe_invalidate_msg(msg, msg_hash))
    }

    fn calculate_msg_hash(&mut self, msg: &Vote) -> u64 {
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
        #[allow(clippy::as_conversions)]
        let prob = (msg_hash as f64) / (u64::MAX as f64);
        prob <= self.drop_probability
    }

    fn maybe_invalidate_msg(&mut self, mut msg: Vote, msg_hash: u64) -> Vote {
        #[allow(clippy::as_conversions)]
        if (msg_hash as f64) / (u64::MAX as f64) > self.invalid_probability {
            return msg;
        }
        debug!("Invalidating message");
        // TODO(matan): Allow for invalid votes based on signature.
        msg.voter = ContractAddress(PatriciaKey::from(msg_hash));
        msg
    }
}

impl Stream for NetworkReceiver {
    type Item = (Result<Vote, ProtobufConversionError>, BroadcastedMessageMetadata);

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let item = self.broadcasted_messages_receiver.poll_next_unpin(cx);
            let (msg, broadcasted_message_metadata) = match item {
                Poll::Ready(Some((Ok(msg), broadcasted_message_metadata))) => {
                    (msg, broadcasted_message_metadata)
                }
                _ => return item,
            };
            if let Some(msg) = self.filter_msg(msg) {
                return Poll::Ready(Some((Ok(msg), broadcasted_message_metadata)));
            }
        }
    }
}
