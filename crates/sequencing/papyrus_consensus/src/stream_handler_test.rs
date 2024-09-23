use futures::channel::mpsc;
// use papyrus_network::gossipsub_impl::Topic;
// use papyrus_network::network_manager::{
//     BroadcastTopicChannels,
//     GenericNetworkManager,
//     GenericReceiver,
// };
// use papyrus_network::network_manager::test_utils::{
//     create_test_broadcasted_message_manager,
//     mock_register_broadcast_topic,
//     MockBroadcastedMessagesSender,
//     TestSubscriberChannels,
// };
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, StreamMessage};

use super::{MessageId, PeerId, StreamCollector, StreamCollectorConfig, StreamId};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn make_random_message(
        stream_id: StreamId,
        message_id: MessageId,
        fin: bool,
    ) -> StreamMessage<ConsensusMessage> {
        StreamMessage {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id,
            message_id,
            fin,
        }
    }

    // Check if two vectors are the same.
    // ref: https://stackoverflow.com/a/58175659
    fn do_vecs_match<T: PartialEq>(a: &Vec<T>, b: &Vec<T>) -> bool {
        let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
        matching == a.len() && matching == b.len()
    }

    fn setup_test() -> (
        StreamCollector<ConsensusMessage>,
        // External network sender (simulates other components sending in a stream)
        mpsc::Sender<(StreamMessage<ConsensusMessage>, PeerId)>,
        // Internal output (receiver getting the ordered output from StreamCollector)
        mpsc::Receiver<mpsc::Receiver<StreamMessage<ConsensusMessage>>>,
    ) {
        // Set up a fake network to simulate communication with external services, where stream
        // messages come in. TODO(guyn): replace this with a mock network?
        // let TestSubscriberChannels { mock_network, mut subscriber_channels } =
        // mock_register_broadcast_topic().unwrap();
        // let mut external_sender = mock_network.broadcasted_messages_sender;
        // let mut external_receiver = subscriber_channels.broadcasted_messages_receiver;
        let (external_sender, external_receiver) =
            mpsc::channel::<(StreamMessage<ConsensusMessage>, PeerId)>(100);

        // Set up the internal communication channels
        let (internal_sender, internal_receiver) =
            mpsc::channel::<mpsc::Receiver<StreamMessage<ConsensusMessage>>>(100);

        let config = StreamCollectorConfig::default();

        let handler: StreamCollector<ConsensusMessage> =
            StreamCollector::new(config, external_receiver, internal_sender);

        (handler, external_sender, internal_receiver)
    }

    #[tokio::test]
    async fn test_stream_handler_in_order() {
        let (mut h, mut external_sender, mut internal_receiver) = setup_test();
        let peer_id = 42;
        let stream_id = 127;

        for i in 0..10 {
            let message = make_random_message(stream_id, i, i == 9);
            external_sender.try_send((message, peer_id)).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        assert!(h.message_buffers.is_empty());

        let mut first_receiver = internal_receiver
            .try_next()
            .expect("Receive message should succeed")
            .expect("Receive message should succeed");

        for i in 0..10 {
            let message = first_receiver
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id);
            assert_eq!(message.message_id, i);
            if i == 9 {
                assert!(message.fin);
            }
        }
    }

    #[tokio::test]
    async fn test_stream_handler_in_reverse() {
        let (mut h, mut external_sender, mut internal_receiver) = setup_test();
        let peer_id = 42;
        let stream_id = 127;

        for i in 0..5 {
            let message = make_random_message(stream_id, 5 - i, i == 0);
            external_sender.try_send((message, peer_id)).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });
        let mut h = join_handle.await.expect("Task should succeed");
        assert_eq!(h.message_buffers.len(), 1);
        assert_eq!(h.message_buffers[&(peer_id, stream_id)].len(), 5);
        let range: Vec<u64> = (1..6).collect();
        let keys = h.message_buffers[&(peer_id, stream_id)].clone().into_keys().collect();
        assert!(do_vecs_match(&keys, &range));

        // Now send the last message.
        external_sender
            .try_send((make_random_message(stream_id, 0, false), peer_id))
            .expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        assert!(h.message_buffers.is_empty());

        let mut first_receiver = internal_receiver
            .try_next()
            .expect("Receive message should succeed")
            .expect("Receive message should succeed");

        for i in 0..6 {
            let message = first_receiver
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id);
            assert_eq!(message.message_id, i);
            if i == 5 {
                assert!(message.fin);
            }
        }
    }

    #[tokio::test]
    async fn test_stream_handler_multiple_streams() {
        let (mut h, mut external_sender, mut internal_receiver) = setup_test();

        let peer_id1 = 42;
        let stream_id1 = 127; // Send all messages in order (except the first one).
        let peer_id2 = 43;
        let stream_id2 = 10; // Send in reverse order (except the first one).
        let peer_id3 = 44; // Notice that stream_id2==stream_id3 but the peer_id is different.
        let stream_id3 = 10; // Send in two batches of 5 messages, without the first one, don't send fin.

        for i in 1..10 {
            let message = make_random_message(stream_id1, i, i == 9);
            external_sender.try_send((message, peer_id1)).expect("Send should succeed");
        }

        for i in 0..5 {
            let message = make_random_message(stream_id2, 5 - i, i == 0);
            external_sender.try_send((message, peer_id2)).expect("Send should succeed");
        }

        for i in 5..10 {
            let message = make_random_message(stream_id3, i, false);
            external_sender.try_send((message, peer_id3)).expect("Send should succeed");
        }
        for i in 1..5 {
            let message = make_random_message(stream_id3, i, false);
            external_sender.try_send((message, peer_id3)).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        // Make sure all the messages were received.
        let mut h = join_handle.await.expect("Task should succeed");

        // This receiver should produce messages from stream_id1.
        let mut first_receiver = internal_receiver
            .try_next()
            .expect("Receive message should succeed")
            .expect("Receive message should succeed");

        // This receiver should produce messages from stream_id2.
        let mut second_receiver = internal_receiver
            .try_next()
            .expect("Receive message should succeed")
            .expect("Receive message should succeed");

        // This receiver should produce messages from stream_id3.
        let mut third_receiver = internal_receiver
            .try_next()
            .expect("Receive message should succeed")
            .expect("Receive message should succeed");

        // Check the internal structure of the handler h.
        assert!(do_vecs_match(
            &h.message_buffers.clone().into_keys().collect(),
            &vec![(42, 127), (43, 10), (44, 10)]
        ));

        // The first case we have all messages from 1 to 9 buffered into one contiguous sequence.
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id1, stream_id1)].clone().into_keys().collect(),
            &(1..2).collect()
        ));

        // The second case we have all messages from 1 to 5 buffered, each into its own vector
        // (worse case scenario).
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id2, stream_id2)].clone().into_keys().collect(),
            &(1..6).collect()
        ));

        // Third case: two vectors, one with messages 1 to 4 and the other with messages 5 to 9.
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id3, stream_id3)].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // Send the last message on stream_id1.
        external_sender
            .try_send((make_random_message(stream_id1, 0, false), peer_id1))
            .expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id1.
        for i in 0..10 {
            let message = first_receiver
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id1);
            assert_eq!(message.message_id, i);
            if i == 9 {
                assert!(message.fin);
            }
        }

        // Stream_id1 should be gone.
        assert!(do_vecs_match(
            &h.message_buffers.clone().into_keys().collect(),
            &vec![(43, 10), (44, 10)]
        ));

        // The other two streams should be the same as before.
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id2, stream_id2)].clone().into_keys().collect(),
            &(1..6).collect()
        ));
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id3, stream_id3)].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // Send the last message on stream_id2.
        external_sender
            .try_send((make_random_message(stream_id2, 0, false), peer_id2))
            .expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id2.
        for i in 0..6 {
            let message = second_receiver
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id2);
            assert_eq!(message.message_id, i);
            if i == 5 {
                assert!(message.fin);
            }
        }

        // Stream_id2 should also be gone.
        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![(44, 10)]));

        // The last stream should be the same as before.
        assert!(do_vecs_match(
            &h.message_buffers[&(peer_id3, stream_id3)].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // Send the last message on stream_id3.
        external_sender
            .try_send((make_random_message(stream_id3, 0, false), peer_id3))
            .expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        // Should be able to read all the messages for stream_id3.
        let h = join_handle.await.expect("Task should succeed");
        for i in 0..10 {
            let message = third_receiver
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id3);
            assert_eq!(message.message_id, i);
            assert!(!message.fin);
        }

        // Stream_id3 should still be there, because we didn't send a fin.
        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![(44, 10)]));

        // ...but the buffer should be empty, as we've successfully drained it all.
        assert!(h.message_buffers[&(peer_id3, stream_id3)].is_empty());
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_duplicate_message_fails() {
        let (mut h, mut external_sender, _internal_receiver) = setup_test();
        external_sender
            .try_send((make_random_message(13, 42, false), 12))
            .expect("Send should succeed");
        external_sender
            .try_send((make_random_message(13, 42, false), 12))
            .expect("Send should succeed");

        // This should panic since we are sending the same message twice!
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_after_fin_message_fails() {
        let (mut h, mut external_sender, _internal_receiver) = setup_test();
        external_sender
            .try_send((make_random_message(13, 42, true), 12))
            .expect("Send should succeed");
        external_sender
            .try_send((make_random_message(13, 45, false), 12))
            .expect("Send should succeed");

        // This should panic since the fin was received on message_id 42, but we are sending 45.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_after_fin_message_reverse_fails() {
        let (mut h, mut external_sender, _internal_receiver) = setup_test();
        external_sender
            .try_send((make_random_message(13, 45, false), 12))
            .expect("Send should succeed");
        external_sender
            .try_send((make_random_message(13, 42, true), 12))
            .expect("Send should succeed");

        // This should panic since the fin was received on message_id 42, but we are sending 45.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_max_buffer_fails() {
        let (mut h, mut external_sender, _internal_receiver) = setup_test();
        h.config.max_buffer_size = Some(10);
        // Skip the first message, so the messages all get buffered.
        for i in 0..11 {
            external_sender
                .try_send((make_random_message(13, i + 1, false), 12))
                .expect("Send should succeed");
        }

        // This should panic since there are too many buffered messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_max_streams_fails() {
        let (mut h, mut external_sender, _internal_receiver) = setup_test();
        h.config.max_num_streams = Some(10);
        // Skip the first message, so the messages all get buffered.
        for i in 0..11 {
            external_sender
                .try_send((make_random_message(i, 1, false), i))
                .expect("Send should succeed");
        }
        // This should panic since there are too many streams at the same time.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");
    }
}
