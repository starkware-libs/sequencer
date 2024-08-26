use futures::{SinkExt, StreamExt};
use papyrus_protobuf::consensus::ConsensusMessage;
use test_case::test_case;

use super::NetworkReceiver;

const CACHE_SIZE: usize = 10;
const SEED: u64 = 123;
const DROP_PROBABILITY: f64 = 0.5;
const INVALID_PROBABILITY: f64 = 0.5;

#[test_case(true, true; "distinct_vote")]
#[test_case(false, true; "repeat_vote")]
#[test_case(true, false; "distinct_proposal")]
#[test_case(false, false; "repeat_proposal")]
#[tokio::test]
async fn test_invalid(distinct_messages: bool, is_vote: bool) {
    let (mut sender, receiver) = futures::channel::mpsc::unbounded();
    let mut receiver = NetworkReceiver::new(receiver, CACHE_SIZE, SEED, 0.0, INVALID_PROBABILITY);
    let mut invalid_messages = 0;

    for height in 0..1000 {
        let msg = if is_vote {
            let mut vote = papyrus_protobuf::consensus::Vote::default();
            if distinct_messages {
                vote.height = height;
            }
            ConsensusMessage::Vote(vote.clone())
        } else {
            let mut proposal = papyrus_protobuf::consensus::Proposal::default();
            if distinct_messages {
                proposal.height = height;
            }
            ConsensusMessage::Proposal(proposal.clone())
        };

        let report_sender = futures::channel::oneshot::channel().0;
        sender.send((Ok(msg.clone()), report_sender)).await.unwrap();
        if receiver.next().await.unwrap().0.unwrap() != msg {
            invalid_messages += 1;
        }
    }
    assert!((400..=600).contains(&invalid_messages), "num_invalid={invalid_messages}");
}

#[test_case(true, true; "distinct_vote")]
#[test_case(false, true; "repeat_vote")]
#[test_case(true, false; "distinct_proposal")]
#[test_case(false, false; "repeat_proposal")]
#[tokio::test]
async fn test_drops(distinct_messages: bool, is_vote: bool) {
    let (mut sender, receiver) = futures::channel::mpsc::unbounded();
    let mut receiver = NetworkReceiver::new(receiver, CACHE_SIZE, SEED, DROP_PROBABILITY, 0.0);
    let mut num_received = 0;

    for height in 0..1000 {
        let msg = if is_vote {
            let mut vote = papyrus_protobuf::consensus::Vote::default();
            if distinct_messages {
                vote.height = height;
            }
            ConsensusMessage::Vote(vote.clone())
        } else {
            let mut proposal = papyrus_protobuf::consensus::Proposal::default();
            if distinct_messages {
                proposal.height = height;
            }
            ConsensusMessage::Proposal(proposal.clone())
        };

        let report_sender = futures::channel::oneshot::channel().0;
        sender.send((Ok(msg.clone()), report_sender)).await.unwrap();
    }
    drop(sender);

    while receiver.next().await.is_some() {
        num_received += 1;
    }
    assert!((400..=600).contains(&num_received), "num_received={num_received}");
}
