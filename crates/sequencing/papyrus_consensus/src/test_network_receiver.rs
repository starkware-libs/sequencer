use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::task::Poll;

use futures::{Stream, StreamExt};
use lru::LruCache;
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::ConsensusMessage;
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::BlockHash;

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
pub struct NetworkReceiver<ReceiverT> {
    pub receiver: ReceiverT,
    // Cache is used so that repeat sends of a message can be processed differently. For example,
    // if a message is dropped resending it should result in a new decision.
    pub cache: LruCache<ConsensusMessage, u32>,
    pub seed: u64,
    // Probability of dropping a message [0, 1].
    pub drop_probability: f64,
    // Probability of making a message invalid [0, 1].
    pub invalid_probability: f64,
}

impl<ReceiverT> NetworkReceiver<ReceiverT>
where
    ReceiverT: Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)>,
{
    pub fn new(
        receiver: ReceiverT,
        cache_size: usize,
        seed: u64,
        drop_probability: f64,
        invalid_probability: f64,
    ) -> Self {
        assert!(0.0 <= drop_probability && drop_probability <= 1.0);
        assert!(0.0 <= invalid_probability && invalid_probability <= 1.0);
        Self {
            receiver,
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
    pub fn filter_msg(&mut self, mut msg: ConsensusMessage) -> Option<ConsensusMessage> {
        if !matches!(msg, ConsensusMessage::Proposal(_)) {
            // TODO(matan): Add support for dropping/invalidating votes.
            return Some(msg);
        }

        if self.should_drop_msg(&msg) {
            return None;
        }

        if self.should_invalidate_msg(&msg) {
            self.invalidate_msg(&mut msg);
        }
        Some(msg)
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

    fn should_drop_msg(&mut self, msg: &ConsensusMessage) -> bool {
        let prob = (self.calculate_msg_hash(&msg) as f64) / (u64::MAX as f64);
        prob <= self.drop_probability
    }

    fn should_invalidate_msg(&mut self, msg: &ConsensusMessage) -> bool {
        let prob = (self.calculate_msg_hash(&msg) as f64) / (u64::MAX as f64);
        prob <= self.invalid_probability
    }

    fn invalidate_msg(&mut self, msg: &mut ConsensusMessage) {
        match msg {
            ConsensusMessage::Proposal(ref mut proposal) => {
                proposal.block_hash = BlockHash(proposal.block_hash.0 + 1);
            }
            // TODO(matan): Allow for invalid votes based on signatures.
            _ => {}
        }
    }
}

impl<ReceiverT> Stream for NetworkReceiver<ReceiverT>
where
    ReceiverT:
        Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)> + Unpin,
{
    type Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender);

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let item = self.receiver.poll_next_unpin(cx);
            let (msg, report_sender) = match item {
                Poll::Ready(Some((Ok(msg), report_sender))) => (msg, report_sender),
                _ => return item,
            };
            if let Some(msg) = self.filter_msg(msg) {
                return Poll::Ready(Some((Ok(msg), report_sender)));
            }
        }
    }
}

// Tests for the NetworkReceiver, which is itself a test utility.
mod test {

    use futures::{SinkExt, StreamExt};
    use papyrus_protobuf::consensus::ConsensusMessage;
    use test_case::test_case;

    use super::NetworkReceiver;

    const CACHE_SIZE: usize = 10;
    const SEED: u64 = 123;
    const DROP_PROBABILITY: f64 = 0.5;
    const INVALID_PROBABILITY: f64 = 0.5;

    #[test_case(true; "distinct_messages")]
    #[test_case(false; "repeat_messages")]
    #[tokio::test]
    async fn test_invalid(distinct_messages: bool) {
        let (mut sender, receiver) = futures::channel::mpsc::unbounded();
        let mut receiver =
            NetworkReceiver::new(receiver, CACHE_SIZE, SEED, 0.0, INVALID_PROBABILITY);
        let mut invalid_messages = 0;
        for height in 0..100 {
            let mut proposal = papyrus_protobuf::consensus::Proposal::default();
            if distinct_messages {
                proposal.height = height;
            }
            let report_sender = futures::channel::oneshot::channel().0;
            let msg = ConsensusMessage::Proposal(proposal.clone());
            sender.send((Ok(msg.clone()), report_sender)).await.unwrap();
            if receiver.next().await.unwrap().0.unwrap() != msg {
                invalid_messages += 1;
            }
        }
        assert!(30 <= invalid_messages && invalid_messages <= 70, "num_invalid={invalid_messages}");
    }

    #[test_case(true; "distinct_messages")]
    #[test_case(false; "repeat_messages")]
    #[tokio::test]
    async fn test_drops(distinct_messages: bool) {
        let (mut sender, receiver) = futures::channel::mpsc::unbounded();
        let mut receiver = NetworkReceiver::new(receiver, CACHE_SIZE, SEED, DROP_PROBABILITY, 0.0);
        let mut num_received = 0;
        for height in 0..100 {
            let mut proposal = papyrus_protobuf::consensus::Proposal::default();
            if distinct_messages {
                proposal.height = height;
            }
            let report_sender = futures::channel::oneshot::channel().0;
            let msg = ConsensusMessage::Proposal(proposal.clone());
            sender.send((Ok(msg.clone()), report_sender)).await.unwrap();
        }
        drop(sender);

        while receiver.next().await.is_some() {
            num_received += 1;
        }
        assert!(30 <= num_received && num_received <= 70, "num_received={num_received}");
    }
}
