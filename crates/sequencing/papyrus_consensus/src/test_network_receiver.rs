use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::task::Poll;

use futures::{Stream, StreamExt};
use papyrus_network::network_manager::ReportSender;
use papyrus_protobuf::consensus::ConsensusMessage;
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::BlockHash;

/// A simple cache used to count the occurrences of a key. It is constant size and simply overwrites
/// keys when they overlap (resetting their count).
pub struct Cache {
    data: Vec<Option<(u64, u32)>>,
}

impl Cache {
    fn new(size: usize) -> Self {
        Self { data: vec![None; size] }
    }

    fn insert(&mut self, key: u64) -> u32 {
        let index = key % (self.data.len() as u64);
        let entry = self.data.get_mut(index as usize).unwrap();
        match entry {
            Some((k, count)) if *k == key => {
                *count += 1;
                *count
            }
            Some(_) | None => {
                *entry = Some((key, 1));
                1
            }
        }
    }
}

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
    pub cache: Cache,
    pub seed: u64,
    pub drop_probability: u64,
    pub invalid_probability: u64,
}

impl<ReceiverT> NetworkReceiver<ReceiverT>
where
    ReceiverT: Stream<Item = (Result<ConsensusMessage, ProtobufConversionError>, ReportSender)>,
{
    pub fn new(
        receiver: ReceiverT,
        seed: u64,
        invalid_probability: u64,
        drop_probability: u64,
        cache_size: usize,
    ) -> Self {
        assert!(invalid_probability <= 100);
        assert!(drop_probability <= 100);
        Self {
            receiver,
            cache: Cache::new(cache_size),
            seed,
            drop_probability,
            invalid_probability,
        }
    }

    /// Determine how to handle a message. If None then the message is silently droppeds. If some,
    /// the returned message is what is sent to the consensus crate.
    pub fn filter_msg(&mut self, mut msg: ConsensusMessage) -> Option<ConsensusMessage> {
        if !matches!(msg, ConsensusMessage::Proposal(_)) {
            // TODO(matan): Add support for dropping/invalidating votes.
            return Some(msg);
        }

        let randint = self.calculate_msg_hash(&msg) % 100;
        if randint < self.drop_probability {
            return None;
        }

        let randint = self.calculate_msg_hash(&msg) % 100;
        if randint < self.invalid_probability {
            self.invalidate_msg(&mut msg);
        }
        Some(msg)
    }

    fn calculate_msg_hash(&mut self, msg: &ConsensusMessage) -> u64 {
        let mut hasher = DefaultHasher::new();
        msg.hash(&mut hasher);
        let hash = hasher.finish();

        let count = self.cache.insert(hash);

        let mut hasher = DefaultHasher::new();
        hash.hash(&mut hasher);
        self.seed.hash(&mut hasher);
        count.hash(&mut hasher);
        hasher.finish()
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

#[cfg(test)]
mod test {

    use futures::SinkExt;
    use test_case::test_case;

    use super::*;

    #[test_case(true; "distinct_messages")]
    #[test_case(false; "repeat_messages")]
    #[tokio::test]
    async fn test_invalid(distinct_messages: bool) {
        let (mut sender, receiver) = futures::channel::mpsc::unbounded();
        let mut receiver = NetworkReceiver::new(receiver, 123, 50, 0, 10);
        let mut invalid = 0;
        for height in 0..100 {
            let mut proposal = papyrus_protobuf::consensus::Proposal::default();
            if distinct_messages {
                proposal.height = height;
            }
            let report_sender = futures::channel::oneshot::channel().0;
            let msg = ConsensusMessage::Proposal(proposal.clone());
            sender.send((Ok(msg.clone()), report_sender)).await.unwrap();
            if receiver.next().await.unwrap().0.unwrap() != msg {
                invalid += 1;
            }
        }
        assert!(40 <= invalid && invalid <= 60, "num_invalid={invalid}");
    }

    #[test_case(true; "distinct_messages")]
    #[test_case(false; "repeat_messages")]
    #[tokio::test]
    async fn test_drops(distinct_messages: bool) {
        let (mut sender, receiver) = futures::channel::mpsc::unbounded();
        let mut receiver = NetworkReceiver::new(receiver, 123, 0, 50, 10);
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
        assert!(40 <= num_received && num_received <= 60, "num_received={num_received}");
    }
}
