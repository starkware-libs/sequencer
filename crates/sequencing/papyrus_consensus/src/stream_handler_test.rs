use std::time::Duration;

use futures::channel::mpsc;
use futures::stream::StreamExt;
use futures::SinkExt;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    MockBroadcastedMessagesSender,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, StreamMessage, StreamMessageBody};
use papyrus_test_utils::{get_rng, GetTestInstance};

use super::{get_metadata_peer_id, StreamHandler};

#[cfg(test)]
mod tests {

    use papyrus_network_types::network_types::BroadcastedMessageManager;

    use super::*;

    fn make_test_message(
        stream_id: u64,
        message_id: u64,
        fin: bool,
    ) -> StreamMessage<ConsensusMessage> {
        let content = match fin {
            true => StreamMessageBody::Fin,
            false => StreamMessageBody::Content(ConsensusMessage::Proposal(Proposal::default())),
        };
        StreamMessage { message: content, stream_id, message_id }
    }

    // Check if two vectors are the same:
    fn do_vecs_match<T: PartialEq>(a: &[T], b: &[T]) -> bool {
        let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
        matching == a.len() && matching == b.len()
    }

    async fn send(
        sender: &mut MockBroadcastedMessagesSender<StreamMessage<ConsensusMessage>>,
        metadata: &BroadcastedMessageManager,
        msg: StreamMessage<ConsensusMessage>,
    ) {
        sender.send((msg, metadata.clone())).await.unwrap();
    }

    fn setup_test() -> (
        StreamHandler<ConsensusMessage>,
        MockBroadcastedMessagesSender<StreamMessage<ConsensusMessage>>,
        mpsc::Receiver<mpsc::Receiver<ConsensusMessage>>,
        BroadcastedMessageManager,
        mpsc::Sender<(u64, mpsc::Receiver<ConsensusMessage>)>,
    ) {
        // This (network_broadcast_sender) is the network connector for broadcasting messages to the
        // network.
        let TestSubscriberChannels { mock_network: _, subscriber_channels } =
            mock_register_broadcast_topic().unwrap();
        let BroadcastTopicChannels {
            broadcasted_messages_receiver: _,
            broadcast_topic_client: network_broadcast_sender,
        } = subscriber_channels;

        // This channel is used to feed receivers of messages to StreamHandler for broadcasting.
        // The receiver goes into the StreamHandler, the sender is used by the test (e.g., as mock
        // Consensus) Note that each new channel comes in a tuple with (stream_id,
        // receiver).
        let (broadcast_channel_sender, broadcast_channel_receiver) =
            mpsc::channel::<(u64, mpsc::Receiver<ConsensusMessage>)>(100);

        // This is the network connector for listening to messages from the network.
        let TestSubscriberChannels { mock_network, subscriber_channels } =
            mock_register_broadcast_topic().unwrap();

        // This sender mocks the messages coming in from the network,
        // while the broadcasted_messages_receiver is given to StreamHandler to listen.
        let network_sender_to_listen = mock_network.broadcasted_messages_sender;
        let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client: _ } =
            subscriber_channels;

        // The listen_channel_sender is given to StreamHandler so it can output new channels for
        // each stream. The listen_channel_receiver is given to the "mock consensus" that
        // gets new channels and listens to them.
        let (listen_channel_sender, listen_channel_receiver) =
            mpsc::channel::<mpsc::Receiver<ConsensusMessage>>(100);

        // TODO(guyn): We should also give the broadcast_topic_client to the StreamHandler
        let handler = StreamHandler::new(
            listen_channel_sender,
            broadcasted_messages_receiver,
            network_broadcast_sender,
            broadcast_channel_receiver,
        );

        let listen_metadata = BroadcastedMessageManager::get_test_instance(&mut get_rng());

        (
            handler,
            network_sender_to_listen,
            listen_channel_receiver,
            listen_metadata,
            broadcast_channel_sender,
        )
    }

    #[tokio::test]
    async fn stream_handler_listen_in_order() {
        let (mut stream_handler, mut network_sender, mut listen_channel_receiver, metadata, _) =
            setup_test();

        let stream_id = 127;
        for i in 0..10 {
            let message = make_test_message(stream_id, i, i == 9);
            send(&mut network_sender, &metadata, message).await;
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
        });

        join_handle.await.expect("Task should succeed");

        let mut receiver = listen_channel_receiver.next().await.unwrap();
        for _ in 0..9 {
            // message number 9 is Fin, so it will not be sent!
            let _ = receiver.next().await.unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn stream_handler_listen_in_reverse() {
        let (
            mut stream_handler,
            mut network_sender,
            mut listen_channel_receiver,
            listen_metadata,
            _,
        ) = setup_test();
        let peer_id = get_metadata_peer_id(&listen_metadata);
        let stream_id = 127;

        for i in 0..5 {
            let message = make_test_message(stream_id, 5 - i, i == 0);
            send(&mut network_sender, &listen_metadata, message).await;
        }
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Get the receiver for the stream.
        let mut receiver = listen_channel_receiver.next().await.unwrap();
        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver.try_next().is_err());

        assert_eq!(stream_handler.listen_stream_data.len(), 1);
        assert_eq!(
            stream_handler.listen_stream_data[&(peer_id.clone(), stream_id)].message_buffer.len(),
            5
        );
        let range: Vec<u64> = (1..6).collect();
        let keys: Vec<u64> = stream_handler.listen_stream_data[&(peer_id, stream_id)]
            .clone()
            .message_buffer
            .into_keys()
            .collect();
        assert!(do_vecs_match(&keys, &range));

        // Now send the last message:
        send(&mut network_sender, &listen_metadata, make_test_message(stream_id, 0, false)).await;
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });

        let stream_handler = join_handle.await.expect("Task should succeed");
        assert!(stream_handler.listen_stream_data.is_empty());

        for _ in 0..5 {
            // message number 5 is Fin, so it will not be sent!
            let _ = receiver.next().await.unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn stream_handler_listen_multiple_streams() {
        let (
            mut stream_handler,
            mut network_sender,
            mut listen_channel_receiver,
            listen_metadata,
            _,
        ) = setup_test();
        let peer_id = get_metadata_peer_id(&listen_metadata);

        let stream_id1 = 127; // Send all messages in order (except the first one).
        let stream_id2 = 10; // Send in reverse order (except the first one).
        let stream_id3 = 1; // Send in two batches, without the first one, don't send fin.

        for i in 1..10 {
            let message = make_test_message(stream_id1, i, i == 9);
            send(&mut network_sender, &listen_metadata, message).await;
        }

        for i in 0..5 {
            let message = make_test_message(stream_id2, 5 - i, i == 0);
            send(&mut network_sender, &listen_metadata, message).await;
        }

        for i in 5..10 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &listen_metadata, message).await;
        }
        for i in 1..5 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &listen_metadata, message).await;
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        let values = vec![(peer_id.clone(), 1), (peer_id.clone(), 10), (peer_id.clone(), 127)];
        assert!(
            stream_handler
                .listen_stream_data
                .clone()
                .into_keys()
                .all(|item| values.contains(&item))
        );

        // We have all message from 1 to 9 buffered.
        assert!(do_vecs_match(
            &stream_handler.listen_stream_data[&(peer_id.clone(), stream_id1)]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &stream_handler.listen_stream_data[&(peer_id.clone(), stream_id2)]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..6).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &stream_handler.listen_stream_data[&(peer_id.clone(), stream_id3)]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // Get the receiver for the first stream.
        let mut receiver1 = listen_channel_receiver.next().await.unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver1.try_next().is_err());

        // Get the receiver for the second stream.
        let mut receiver2 = listen_channel_receiver.next().await.unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver2.try_next().is_err());

        // Get the receiver for the third stream.
        let mut receiver3 = listen_channel_receiver.next().await.unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver3.try_next().is_err());

        // Send the last message on stream_id1:
        send(&mut network_sender, &listen_metadata, make_test_message(stream_id1, 0, false)).await;
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id1.
        for _ in 0..9 {
            // message number 9 is Fin, so it will not be sent!
            let _ = receiver1.next().await.unwrap();
        }

        // Check that the receiver was closed:
        assert!(matches!(receiver1.try_next(), Ok(None)));

        // stream_id1 should be gone
        let values = vec![(peer_id.clone(), 1), (peer_id.clone(), 10)];
        assert!(
            stream_handler
                .listen_stream_data
                .clone()
                .into_keys()
                .all(|item| values.contains(&item))
        );

        // Send the last message on stream_id2:
        send(&mut network_sender, &listen_metadata, make_test_message(stream_id2, 0, false)).await;
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id2.
        for _ in 0..5 {
            // message number 5 is Fin, so it will not be sent!
            let _ = receiver2.next().await.unwrap();
        }

        // Check that the receiver was closed:
        assert!(matches!(receiver2.try_next(), Ok(None)));

        // Stream_id2 should also be gone.
        let values = vec![(peer_id.clone(), 1)];
        assert!(
            stream_handler
                .listen_stream_data
                .clone()
                .into_keys()
                .all(|item| values.contains(&item))
        );

        // Send the last message on stream_id3:
        send(&mut network_sender, &listen_metadata, make_test_message(stream_id3, 0, false)).await;

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.run()).await;
            stream_handler
        });

        let stream_handler = join_handle.await.expect("Task should succeed");
        for _ in 0..10 {
            // All messages are received, including number 9 which is not Fin
            let _ = receiver3.next().await.unwrap();
        }

        // In this case the receiver is not closed, because we didn't send a fin.
        assert!(matches!(receiver3.try_next(), Err(_)));

        // Stream_id3 should still be there, because we didn't send a fin.
        let values = vec![(peer_id.clone(), 1)];
        assert!(
            stream_handler
                .listen_stream_data
                .clone()
                .into_keys()
                .all(|item| values.contains(&item))
        );

        // But the buffer should be empty, as we've successfully drained it all.
        assert!(
            stream_handler.listen_stream_data[&(peer_id, stream_id3)].message_buffer.is_empty()
        );
    }
}
