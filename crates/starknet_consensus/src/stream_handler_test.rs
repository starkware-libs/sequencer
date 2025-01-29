use std::fmt::Display;
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
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::{StreamMessage, StreamMessageBody};
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_test_utils::{get_rng, GetTestInstance};
use prost::DecodeError;

use super::{MessageId, StreamHandler, MAX_MESSAGES_PER_STREAM, MAX_STREAMS_PER_PEER};

const TIMEOUT: Duration = Duration::from_millis(100);
const CHANNEL_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TestStreamId(u64);

impl From<TestStreamId> for Vec<u8> {
    fn from(value: TestStreamId) -> Self {
        value.0.to_be_bytes().to_vec()
    }
}

impl TryFrom<Vec<u8>> for TestStreamId {
    type Error = ProtobufConversionError;
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if bytes.len() != 8 {
            return Err(ProtobufConversionError::DecodeError(DecodeError::new("Invalid length")));
        }
        let mut array = [0; 8];
        array.copy_from_slice(&bytes);
        Ok(TestStreamId(u64::from_be_bytes(array)))
    }
}

impl PartialOrd for TestStreamId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TestStreamId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl Display for TestStreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

mod tests {
    use std::collections::HashMap;

    use papyrus_protobuf::consensus::{IntoFromProto, ProposalInit, ProposalPart};

    use super::*;

    fn make_test_message(
        stream_id: TestStreamId,
        message_id: MessageId,
        fin: bool,
    ) -> StreamMessage<ProposalPart, TestStreamId> {
        let content = match fin {
            true => StreamMessageBody::Fin,
            false => StreamMessageBody::Content(ProposalPart::Init(ProposalInit::default())),
        };
        StreamMessage { message: content, stream_id, message_id }
    }

    fn make_test_message_with_stream_id(
        stream_id: TestStreamId,
        message_id: MessageId,
        fin: bool,
    ) -> StreamMessage<TestStreamId, TestStreamId> {
        let content = match fin {
            true => StreamMessageBody::Fin,
            false => StreamMessageBody::Content(stream_id),
        };
        StreamMessage { message: content, stream_id, message_id }
    }

    // Check if two vectors are the same, regardless of ordering
    fn do_vecs_match_unordered<T>(a: &[T], b: &[T]) -> bool
    where
        T: std::hash::Hash + Eq + PartialEq + Ord + Clone,
    {
        let mut a = a.to_owned();
        a.sort();
        let mut b = b.to_owned();
        b.sort();
        a == b
    }

    async fn send<T: IntoFromProto>(
        sender: &mut MockBroadcastedMessagesSender<StreamMessage<T, TestStreamId>>,
        metadata: &BroadcastedMessageMetadata,
        msg: StreamMessage<T, TestStreamId>,
    ) {
        sender.send((msg, metadata.clone())).await.unwrap();
    }

    #[allow(clippy::type_complexity)]
    fn setup_test<T>() -> (
        StreamHandler<T, TestStreamId>,
        MockBroadcastedMessagesSender<StreamMessage<T, TestStreamId>>,
        mpsc::Receiver<mpsc::Receiver<T>>,
        BroadcastedMessageMetadata,
        mpsc::Sender<(TestStreamId, mpsc::Receiver<T>)>,
        futures::stream::Map<
            mpsc::Receiver<Vec<u8>>,
            fn(Vec<u8>) -> StreamMessage<T, TestStreamId>,
        >,
    )
    where
        T: IntoFromProto + Clone + Send + 'static,
    {
        // The outbound_sender is the network connector for broadcasting messages.
        // The network_broadcast_receiver is used to catch those messages in the test.
        let TestSubscriberChannels { mock_network: mock_broadcast_network, subscriber_channels } =
            mock_register_broadcast_topic().unwrap();
        let BroadcastTopicChannels {
            broadcasted_messages_receiver: _,
            broadcast_topic_client: outbound_sender,
        } = subscriber_channels;

        let network_broadcast_receiver = mock_broadcast_network.messages_to_broadcast_receiver;

        // This is used to feed receivers of messages to StreamHandler for broadcasting.
        // The receiver goes into StreamHandler, sender is used by the test (as mock Consensus).
        // Note that each new channel comes in a tuple with (stream_id, receiver).
        let (outbound_channel_sender, outbound_channel_receiver) =
            mpsc::channel::<(TestStreamId, mpsc::Receiver<T>)>(CHANNEL_SIZE);

        // The network_sender_to_inbound is the sender of the mock network, that is used by the
        // test to send messages into the StreamHandler (from the mock network).
        let TestSubscriberChannels { mock_network, subscriber_channels } =
            mock_register_broadcast_topic().unwrap();
        let network_sender_to_inbound = mock_network.broadcasted_messages_sender;

        // The inbound_receiver is given to StreamHandler to mock network messages.
        let BroadcastTopicChannels {
            broadcasted_messages_receiver: inbound_receiver,
            broadcast_topic_client: _,
        } = subscriber_channels;

        // The inbound_channel_sender is given to StreamHandler so it can output new channels for
        // each stream. The inbound_channel_receiver is given to the "mock consensus" that
        // gets new channels and inbounds to them.
        let (inbound_channel_sender, inbound_channel_receiver) =
            mpsc::channel::<mpsc::Receiver<T>>(CHANNEL_SIZE);

        // TODO(guyn): We should also give the broadcast_topic_client to the StreamHandler
        // This will allow reporting to the network things like bad peers.
        let handler = StreamHandler::new(
            inbound_channel_sender,
            inbound_receiver,
            outbound_channel_receiver,
            outbound_sender,
        );

        let inbound_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

        (
            handler,
            network_sender_to_inbound,
            inbound_channel_receiver,
            inbound_metadata,
            outbound_channel_sender,
            network_broadcast_receiver,
        )
    }

    #[tokio::test]
    async fn inbound_in_order() {
        let (mut stream_handler, mut network_sender, mut inbound_channel_receiver, metadata, _, _) =
            setup_test();

        let stream_id = TestStreamId(127);
        for i in 0..10 {
            let message = make_test_message(stream_id, i, i == 9);
            send(&mut network_sender, &metadata, message).await;
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
        });

        join_handle.await.expect("Task should succeed");

        let mut receiver = inbound_channel_receiver.next().await.unwrap();
        for _ in 0..9 {
            // message number 9 is Fin, so it will not be sent!
            let _ = receiver.next().await.unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn inbound_in_reverse() {
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            inbound_metadata,
            _,
            _,
        ) = setup_test();
        let peer_id = inbound_metadata.originator_id.clone();
        let stream_id = TestStreamId(127);

        for i in 0..5 {
            let message = make_test_message(stream_id, 5 - i, i == 0);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // No receiver should be created yet.
        assert!(inbound_channel_receiver.try_next().is_err());

        assert_eq!(stream_handler.inbound_stream_data.len(), 1);
        assert_eq!(
            stream_handler.inbound_stream_data[&peer_id]
                .peek(&stream_id)
                .unwrap()
                .message_buffer
                .len(),
            5
        );
        // Still waiting for message 0.
        assert_eq!(
            stream_handler.inbound_stream_data[&peer_id].peek(&stream_id).unwrap().next_message_id,
            0
        );
        // Has a receiver, waiting to be sent when message 0 is received.
        assert!(
            stream_handler.inbound_stream_data[&peer_id]
                .peek(&stream_id)
                .unwrap()
                .receiver
                .is_some()
        );

        let range: Vec<u64> = (1..6).collect();
        let keys: Vec<u64> = stream_handler.inbound_stream_data[&peer_id]
            .peek(&stream_id)
            .unwrap()
            .message_buffer
            .keys()
            .copied()
            .collect();
        assert!(do_vecs_match_unordered(&keys, &range));

        // Now send the last message:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id, 0, false)).await;

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        let stream_handler = join_handle.await.expect("Task should succeed");
        assert!(stream_handler.inbound_stream_data.is_empty());

        // Get the receiver for the stream.
        let mut receiver = inbound_channel_receiver.next().await.unwrap();

        for _ in 0..5 {
            // message number 5 is Fin, so it will not be sent!
            let _ = receiver.next().await.unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn inbound_multiple_streams() {
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            inbound_metadata,
            _,
            _,
        ) = setup_test();
        let peer_id = inbound_metadata.originator_id.clone();

        let stream_id1 = TestStreamId(127); // Send all messages in order (except the first one).
        let stream_id2 = TestStreamId(10); // Send in reverse order (except the first one).
        let stream_id3 = TestStreamId(1); // Send in two batches, without the first one, don't send fin.

        for i in 1..10 {
            let message = make_test_message(stream_id1, i, i == 9);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        for i in 0..5 {
            let message = make_test_message(stream_id2, 5 - i, i == 0);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        for i in 5..10 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        for i in 1..5 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        let values = [TestStreamId(1), TestStreamId(10), TestStreamId(127)];
        for item in values {
            assert!(stream_handler.inbound_stream_data[&peer_id].contains(&item));
        }

        // We have all message from 1 to 9 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&peer_id]
                .peek(&stream_id1)
                .unwrap()
                .message_buffer
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&peer_id]
                .peek(&stream_id2)
                .unwrap()
                .message_buffer
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &(1..6).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&peer_id]
                .peek(&stream_id3)
                .unwrap()
                .message_buffer
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // None of the streams should have emitted a receiver yet.
        assert!(inbound_channel_receiver.try_next().is_err());

        // Send the last message on stream_id1:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id1, 0, false)).await;

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Get the receiver for the first stream.
        let mut receiver1 = inbound_channel_receiver.next().await.unwrap();

        // Should be able to read all the messages for stream_id1.
        for _ in 0..9 {
            // message number 9 is Fin, so it will not be sent!
            let _ = receiver1.next().await.unwrap();
        }
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // stream_id1 should be gone
        let values = [TestStreamId(1), TestStreamId(10)];
        for item in values {
            assert!(stream_handler.inbound_stream_data[&peer_id].contains(&item));
        }
        // Send the last message on stream_id2:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id2, 0, false)).await;

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Get the receiver for the second stream.
        let mut receiver2 = inbound_channel_receiver.next().await.unwrap();

        // Should be able to read all the messages for stream_id2.
        for _ in 0..5 {
            // message number 5 is Fin, so it will not be sent!
            let _ = receiver2.next().await.unwrap();
        }

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Stream_id2 should also be gone.
        assert!(stream_handler.inbound_stream_data[&peer_id].contains(&TestStreamId(1)));

        // Send the last message on stream_id3:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id3, 0, false)).await;

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Get the receiver for the third stream.
        let mut receiver3 = inbound_channel_receiver.next().await.unwrap();

        let stream_handler = join_handle.await.expect("Task should succeed");
        for _ in 0..10 {
            // All messages are received, including number 9 which is not Fin
            let _ = receiver3.next().await.unwrap();
        }

        // Stream_id3 should still be there, because we didn't send a fin.
        assert!(stream_handler.inbound_stream_data[&peer_id].contains(&TestStreamId(1)));

        // But the buffer should be empty, as we've successfully drained it all.
        assert!(
            stream_handler.inbound_stream_data[&peer_id]
                .peek(&TestStreamId(1))
                .unwrap()
                .message_buffer
                .is_empty()
        );
    }

    #[tokio::test]
    async fn inbound_max_streams_per_peer() {
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            inbound_metadata,
            _,
            _,
        ): (_, _, mpsc::Receiver<mpsc::Receiver<TestStreamId>>, _, _, _) = setup_test();

        // Send too many streams from the same peer. Send messages 1 to 3 on all channels.
        // Note that message 3 is Fin and doesn't get to the receiver (it closes it!).
        // Channel 0 is the last one sent, so it should be dropped when reaching the last channel.
        // Then send 0 on all channels, in reverse order, which will release the buffered messages.
        // Sending in reverse order of streams will mean none of the streams 1 to
        // MAX_STREAMS_PER_PEER will get dropped. When stream 0 gets message 0,
        // it will be a new stream (dropping stream number MAX_STREAMS_PER_PEER) but
        // it will only have message 0, we will not get messages 1 and 2.
        let stream_ids = (0..u64::try_from(MAX_STREAMS_PER_PEER.get()).unwrap() + 1)
            .map(TestStreamId)
            .collect::<Vec<_>>();

        for stream_id in stream_ids.iter() {
            for i in 0..3 {
                let message = make_test_message_with_stream_id(*stream_id, 3 - i, i == 0);
                send(&mut network_sender, &inbound_metadata, message).await;
            }
        }
        for stream_id in stream_ids.iter().rev() {
            let message = make_test_message_with_stream_id(*stream_id, 0, false);
            send(&mut network_sender, &inbound_metadata, message).await;
        }
        // Run the loop for a short duration to process the messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        let _stream_handler = join_handle.await.expect("Task should succeed");

        let mut message_count = HashMap::new();
        let mut stream_state = HashMap::new();
        for _ in stream_ids.iter() {
            // Get the receiver for each stream.
            let mut receiver = inbound_channel_receiver.next().await.unwrap();
            let mut stream_id = u64::MAX;
            for _ in 0..3 {
                // Make sure each channel sends the correct stream_id.
                if let Ok(Some(message)) = receiver.try_next() {
                    stream_id = message.0;
                    message_count.entry(stream_id).and_modify(|e| *e += 1).or_insert(1);
                }
            }
            stream_state.insert(stream_id, receiver.try_next());
        }

        // What should become of the each stream?
        for stream_id in stream_ids.iter() {
            match stream_id.0 {
                0 => {
                    // This stream was reopened, but it should only have one message, and left open.
                    assert_eq!(message_count[&0], 1);
                    assert!(stream_state[&0].is_err());
                }
                id => {
                    // The rest of the channels should have successfully received all three
                    // messages, and closed after receiving the Fin message.
                    assert_eq!(message_count[&id], 3);
                    assert!(matches!(stream_state[&id], Ok(None)));
                }
            }
        }
    }

    #[tokio::test]
    async fn inbound_max_messages_per_stream() {
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            inbound_metadata,
            _,
            _,
        ) = setup_test();
        let stream_id = TestStreamId(127);
        let max_messages_per_stream = u64::try_from(MAX_MESSAGES_PER_STREAM).unwrap();

        // Send MAX_MESSAGES_PER_STREAM messages, without first message.
        for i in 1..=max_messages_per_stream {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        // Send the first message, which should flush the buffer, so we receive all messages.
        let message = make_test_message(stream_id, 0, false);
        send(&mut network_sender, &inbound_metadata, message).await;

        // Run the loop for a short duration to process the messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Get the receiver for the stream, read all messages.
        let mut receiver = inbound_channel_receiver.try_next().unwrap().unwrap();
        for _ in 0..=max_messages_per_stream {
            let _ = receiver.try_next().unwrap().unwrap();
        }

        // Stream should remain open.
        assert!(receiver.try_next().is_err());

        // Send MAX_MESSAGES_PER_STREAM messages, without first message.
        for i in 1..=max_messages_per_stream {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
        }

        // Send another message to the end of the stream (this drops the stream).
        let message = make_test_message(stream_id, max_messages_per_stream + 1, false);
        send(&mut network_sender, &inbound_metadata, message).await;

        // Send the first message, which should flush the buffer, but it was dropped,
        // so we will end up only getting the first message.
        let message = make_test_message(stream_id, 0, false);
        send(&mut network_sender, &inbound_metadata, message).await;

        // Run the loop for a short duration to process the messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        let _stream_handler = join_handle.await.expect("Task should succeed");

        // Get the receiver for the stream.
        let mut receiver = inbound_channel_receiver.next().await.unwrap();
        let _ = receiver.try_next().unwrap().unwrap();

        // Stream should remain open.
        assert!(receiver.try_next().is_err());
    }

    #[tokio::test]
    async fn inbound_close_channel() {
        let (mut stream_handler, mut network_sender, mut inbound_channel_receiver, metadata, _, _) =
            setup_test();

        let stream_id = TestStreamId(127);
        // Send two messages, no Fin.
        for i in 0..2 {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &metadata, message).await;
        }

        // Allow the StreamHandler to process the messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        let mut receiver = inbound_channel_receiver.next().await.unwrap();
        for _ in 0..2 {
            let _ = receiver.next().await.unwrap();
        }

        // Check that the stream handler contains the StreamData.
        assert_eq!(stream_handler.inbound_stream_data.len(), 1);
        assert!(stream_handler.inbound_stream_data[&metadata.originator_id].contains(&stream_id));

        // Close the channel.
        drop(receiver);

        // Send more messages.
        // TODO(guyn): if we set this to 2..4 it fails... the last message opens a new StreamData!
        for i in 2..3 {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &metadata, message).await;
        }

        // Allow the StreamHandler to process the messages.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });
        let stream_handler = join_handle.await.expect("Task should succeed");

        // Check that the stream handler no longer contains the StreamData.
        assert_eq!(stream_handler.inbound_stream_data.len(), 0);
    }

    // This test does two things:
    // 1. Opens two outbound channels and checks that messages get correctly sent on both.
    // 2. Closes the first channel and checks that Fin is sent and that the relevant structures
    //    inside the stream handler are cleaned up.
    #[tokio::test]
    async fn outbound_multiple_streams() {
        let (
            mut stream_handler,
            _,
            _,
            _,
            mut broadcast_channel_sender,
            mut broadcasted_messages_receiver,
        ) = setup_test();

        let stream_id1 = TestStreamId(42);
        let stream_id2 = TestStreamId(127);

        // Start a new stream by sending the (stream_id, receiver).
        let (mut sender1, receiver1) = mpsc::channel(CHANNEL_SIZE);
        broadcast_channel_sender.send((stream_id1, receiver1)).await.unwrap();

        // Send a message on the stream.
        let message1 = ProposalPart::Init(ProposalInit::default());
        sender1.send(message1.clone()).await.unwrap();

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Check that message was broadcasted.
        assert_eq!(broadcasted_message.message, StreamMessageBody::Content(message1));
        assert_eq!(broadcasted_message.stream_id, stream_id1);
        assert_eq!(broadcasted_message.message_id, 0);

        // Check that internally, stream_handler holds this receiver.
        assert_eq!(
            stream_handler.outbound_stream_receivers.keys().collect::<Vec<&TestStreamId>>(),
            vec![&stream_id1]
        );
        // Check that the number of messages sent on this stream is 1.
        assert_eq!(stream_handler.outbound_stream_number[&stream_id1], 1);

        // Send another message on the same stream.
        let message2 = ProposalPart::Init(ProposalInit::default());
        sender1.send(message2.clone()).await.unwrap();

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Check that message was broadcasted.
        assert_eq!(broadcasted_message.message, StreamMessageBody::Content(message2));
        assert_eq!(broadcasted_message.stream_id, stream_id1);
        assert_eq!(broadcasted_message.message_id, 1);
        assert_eq!(stream_handler.outbound_stream_number[&stream_id1], 2);

        // Start a new stream by sending the (stream_id, receiver).
        let (mut sender2, receiver2) = mpsc::channel(CHANNEL_SIZE);
        broadcast_channel_sender.send((stream_id2, receiver2)).await.unwrap();

        // Send a message on the stream.
        let message3 = ProposalPart::Init(ProposalInit::default());
        sender2.send(message3.clone()).await.unwrap();

        // Run the loop for a short duration to process the message.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Check that message was broadcasted.
        assert_eq!(broadcasted_message.message, StreamMessageBody::Content(message3));
        assert_eq!(broadcasted_message.stream_id, stream_id2);
        assert_eq!(broadcasted_message.message_id, 0);
        let mut vec1 =
            stream_handler.outbound_stream_receivers.keys().collect::<Vec<&TestStreamId>>();
        vec1.sort();
        let mut vec2 = vec![&stream_id1, &stream_id2];
        vec2.sort();
        do_vecs_match_unordered(&vec1, &vec2);
        assert_eq!(stream_handler.outbound_stream_number[&stream_id2], 1);

        // Close the first channel.
        sender1.close_channel();

        // Run the loop for a short duration to process that the channel was closed.
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(TIMEOUT, stream_handler.run()).await;
            stream_handler
        });

        // Check that we got a fin message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();
        assert_eq!(broadcasted_message.message, StreamMessageBody::Fin);

        let stream_handler = join_handle.await.expect("Task should succeed");

        // Check that the information about this stream is gone.
        assert_eq!(
            stream_handler.outbound_stream_receivers.keys().collect::<Vec<&TestStreamId>>(),
            vec![&stream_id2]
        );
    }
}
