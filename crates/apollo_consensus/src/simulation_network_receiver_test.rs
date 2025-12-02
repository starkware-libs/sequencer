use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    TestSubscriberChannels,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::Vote;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::{SinkExt, StreamExt};
use starknet_api::block::BlockNumber;
use test_case::test_case;

use super::NetworkReceiver;

const CACHE_SIZE: usize = 10;
const SEED: u64 = 123;
const DROP_PROBABILITY: f64 = 0.5;
const INVALID_PROBABILITY: f64 = 0.5;

#[test_case(true; "distinct_vote")]
#[test_case(false; "repeat_vote")]
#[tokio::test]
async fn test_invalid(distinct_messages: bool) {
    let TestSubscriberChannels { subscriber_channels, mut mock_network } =
        mock_register_broadcast_topic().unwrap();
    let mut receiver = NetworkReceiver::new(
        subscriber_channels.broadcasted_messages_receiver,
        CACHE_SIZE,
        SEED,
        0.0,
        INVALID_PROBABILITY,
    );
    let mut invalid_messages = 0;

    for height in 0..1000 {
        let msg = Vote {
            height: if distinct_messages { BlockNumber(height) } else { BlockNumber(0) },
            ..Default::default()
        };
        let broadcasted_message_metadata =
            BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
        mock_network
            .broadcasted_messages_sender
            .send((msg.clone(), broadcasted_message_metadata))
            .await
            .unwrap();
        if receiver.next().await.unwrap().0.unwrap() != msg {
            invalid_messages += 1;
        }
    }
    assert!((400..=600).contains(&invalid_messages), "num_invalid={invalid_messages}");
}

#[test_case(true; "distinct_vote")]
#[test_case(false; "repeat_vote")]
#[tokio::test]
async fn test_drops(distinct_messages: bool) {
    let TestSubscriberChannels { subscriber_channels, mut mock_network } =
        mock_register_broadcast_topic().unwrap();
    let mut receiver = NetworkReceiver::new(
        subscriber_channels.broadcasted_messages_receiver,
        CACHE_SIZE,
        SEED,
        DROP_PROBABILITY,
        0.0,
    );
    let mut num_received = 0;

    for height in 0..1000 {
        let msg = Vote {
            height: if distinct_messages { BlockNumber(height) } else { BlockNumber(0) },
            ..Default::default()
        };
        let broadcasted_message_metadata =
            BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
        mock_network
            .broadcasted_messages_sender
            .send((msg.clone(), broadcasted_message_metadata))
            .await
            .unwrap();
    }
    drop(mock_network.broadcasted_messages_sender);

    while receiver.next().await.is_some() {
        num_received += 1;
    }
    assert!((400..=600).contains(&num_received), "num_received={num_received}");
}
