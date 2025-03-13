use std::fmt::Display;

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

use super::{MessageId, StreamHandler};

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
        write!(f, "TestStreamId({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicServer};
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
        StreamHandler<
            T,
            TestStreamId,
            BroadcastTopicServer<StreamMessage<T, TestStreamId>>,
            BroadcastTopicClient<StreamMessage<T, TestStreamId>>,
        >,
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
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            metadata,
            _x,
            _y,
        ) = setup_test();

        let stream_id = TestStreamId(127);
        for i in 0..10 {
            let message = make_test_message(stream_id, i, i == 9);
            send(&mut network_sender, &metadata, message).await;
            stream_handler.handle_next_msg().await.unwrap();
        }

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
            _x,
            _y,
        ) = setup_test();
        let peer_id = inbound_metadata.originator_id.clone();
        let stream_id = TestStreamId(127);

        for i in 0..5 {
            let message = make_test_message(stream_id, 5 - i, i == 0);
            send(&mut network_sender, &inbound_metadata, message).await;
            stream_handler.handle_next_msg().await.unwrap();
        }

        // No receiver should be created yet.
        assert!(inbound_channel_receiver.try_next().is_err());

        assert_eq!(stream_handler.inbound_stream_data.len(), 1);
        assert_eq!(
            stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id)].message_buffer.len(),
            5
        );
        // Still waiting for message 0.
        assert_eq!(
            stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id)].next_message_id,
            0
        );
        // Has a receiver, waiting to be sent when message 0 is received.
        assert!(
            stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id)].receiver.is_some()
        );

        let range: Vec<u64> = (1..6).collect();
        let keys: Vec<u64> = stream_handler.inbound_stream_data[&(peer_id, stream_id)]
            .message_buffer
            .keys()
            .copied()
            .collect();
        assert!(do_vecs_match_unordered(&keys, &range));

        // Now send the last message:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id, 0, false)).await;
        stream_handler.handle_next_msg().await.unwrap();

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
            _x,
            _y,
        ) = setup_test();
        let peer_id = inbound_metadata.originator_id.clone();

        let stream_id1 = TestStreamId(127); // Send all messages in order (except the first one).
        let stream_id2 = TestStreamId(10); // Send in reverse order (except the first one).
        let stream_id3 = TestStreamId(1); // Send in two batches, without the first one, don't send fin.

        let mut num_msgs = 0;
        for i in 1..10 {
            let message = make_test_message(stream_id1, i, i == 9);
            send(&mut network_sender, &inbound_metadata, message).await;
            num_msgs += 1;
        }

        for i in 0..5 {
            let message = make_test_message(stream_id2, 5 - i, i == 0);
            send(&mut network_sender, &inbound_metadata, message).await;
            num_msgs += 1;
        }

        for i in 5..10 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
            num_msgs += 1;
        }

        for i in 1..5 {
            let message = make_test_message(stream_id3, i, false);
            send(&mut network_sender, &inbound_metadata, message).await;
            num_msgs += 1;
        }

        for _ in 0..num_msgs {
            stream_handler.handle_next_msg().await.unwrap();
        }

        let values = [
            (peer_id.clone(), TestStreamId(1)),
            (peer_id.clone(), TestStreamId(10)),
            (peer_id.clone(), TestStreamId(127)),
        ];
        assert!(
            stream_handler.inbound_stream_data.keys().to_owned().all(|item| values.contains(item))
        );

        // We have all message from 1 to 9 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id1)]
                .message_buffer
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id2)]
                .message_buffer
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &(1..6).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match_unordered(
            &stream_handler.inbound_stream_data[&(peer_id.clone(), stream_id3)]
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
        stream_handler.handle_next_msg().await.unwrap();

        // Get the receiver for the first stream.
        let mut receiver1 = inbound_channel_receiver.next().await.unwrap();

        // Should be able to read all the messages for stream_id1.
        for _ in 0..9 {
            // message number 9 is Fin, so it will not be sent!
            let _ = receiver1.next().await.unwrap();
        }

        // stream_id1 should be gone
        let values = [(peer_id.clone(), TestStreamId(1)), (peer_id.clone(), TestStreamId(10))];
        assert!(
            stream_handler.inbound_stream_data.keys().to_owned().all(|item| values.contains(item))
        );

        // Send the last message on stream_id2:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id2, 0, false)).await;
        stream_handler.handle_next_msg().await.unwrap();

        // Get the receiver for the second stream.
        let mut receiver2 = inbound_channel_receiver.next().await.unwrap();

        // Should be able to read all the messages for stream_id2.
        for _ in 0..5 {
            // message number 5 is Fin, so it will not be sent!
            let _ = receiver2.next().await.unwrap();
        }

        // Stream_id2 should also be gone.
        let values = [(peer_id.clone(), TestStreamId(1))];
        assert!(
            stream_handler.inbound_stream_data.keys().to_owned().all(|item| values.contains(item))
        );

        // Send the last message on stream_id3:
        send(&mut network_sender, &inbound_metadata, make_test_message(stream_id3, 0, false)).await;
        stream_handler.handle_next_msg().await.unwrap();

        // Get the receiver for the third stream.
        let mut receiver3 = inbound_channel_receiver.next().await.unwrap();

        for _ in 0..10 {
            // All messages are received, including number 9 which is not Fin
            let _ = receiver3.next().await.unwrap();
        }

        // Stream_id3 should still be there, because we didn't send a fin.
        let values = [(peer_id.clone(), TestStreamId(1))];
        assert!(
            stream_handler.inbound_stream_data.keys().to_owned().all(|item| values.contains(item))
        );

        // But the buffer should be empty, as we've successfully drained it all.
        assert!(
            stream_handler.inbound_stream_data[&(peer_id, stream_id3)].message_buffer.is_empty()
        );
    }

    #[tokio::test]
    async fn inbound_close_channel() {
        let (
            mut stream_handler,
            mut network_sender,
            mut inbound_channel_receiver,
            metadata,
            _x,
            _y,
        ) = setup_test();

        let stream_id = TestStreamId(127);
        // Send two messages, no Fin.
        for i in 0..2 {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &metadata, message).await;
        }
        for _ in 0..2 {
            stream_handler.handle_next_msg().await.unwrap();
        }

        let mut receiver = inbound_channel_receiver.next().await.unwrap();
        for _ in 0..2 {
            let _ = receiver.next().await.unwrap();
        }

        // Check that the stream handler contains the StreamData.
        assert_eq!(stream_handler.inbound_stream_data.len(), 1);
        assert_eq!(
            stream_handler.inbound_stream_data.keys().next().unwrap(),
            &(metadata.originator_id.clone(), stream_id)
        );

        // Close the channel.
        drop(receiver);

        // Send more messages.
        // TODO(guyn): if we set this to 2..4 it fails... the last message opens a new StreamData!
        for i in 2..3 {
            let message = make_test_message(stream_id, i, false);
            send(&mut network_sender, &metadata, message).await;
            stream_handler.handle_next_msg().await.unwrap();
        }

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
            _x,
            _y,
            _z,
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

        stream_handler.handle_next_msg().await.unwrap(); // New stream.
        stream_handler.handle_next_msg().await.unwrap(); // ProposalInit.

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();

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
        stream_handler.handle_next_msg().await.unwrap();

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();

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

        stream_handler.handle_next_msg().await.unwrap(); // New stream.
        stream_handler.handle_next_msg().await.unwrap(); // ProposalInit.

        // Wait for an incoming message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();

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
        stream_handler.handle_next_msg().await.unwrap(); // Channel closed.

        // Check that we got a fin message.
        let broadcasted_message = broadcasted_messages_receiver.next().await.unwrap();
        assert_eq!(broadcasted_message.message, StreamMessageBody::Fin);

        // Check that the information about this stream is gone.
        assert_eq!(
            stream_handler.outbound_stream_receivers.keys().collect::<Vec<&TestStreamId>>(),
            vec![&stream_id2]
        );
    }
}

mod tests_v2 {
    use std::collections::BTreeSet;

    use futures::channel::mpsc::{self, Receiver, SendError, Sender};
    use futures::{FutureExt, SinkExt, StreamExt};
    use papyrus_network::network_manager::{BroadcastTopicClientTrait, ReceivedBroadcastedMessage};
    use papyrus_network_types::network_types::BroadcastedMessageMetadata;
    use papyrus_protobuf::consensus::{ProposalInit, ProposalPart, StreamMessageBody};
    use papyrus_test_utils::{get_rng, GetTestInstance};

    use super::{TestStreamId, CHANNEL_SIZE};
    use crate::stream_handler::StreamHandler;

    type StreamMessage = papyrus_protobuf::consensus::StreamMessage<ProposalPart, TestStreamId>;

    struct FakeBroadcastClient {
        sender: Sender<StreamMessage>,
    }

    #[async_trait::async_trait]
    impl BroadcastTopicClientTrait<StreamMessage> for FakeBroadcastClient {
        async fn broadcast_message(&mut self, message: StreamMessage) -> Result<(), SendError> {
            self.sender.send(message).await
        }

        async fn report_peer(&mut self, _: BroadcastedMessageMetadata) -> Result<(), SendError> {
            todo!()
        }

        async fn continue_propagation(
            &mut self,
            _: &BroadcastedMessageMetadata,
        ) -> Result<(), SendError> {
            todo!()
        }
    }

    #[allow(clippy::type_complexity)]
    fn setup() -> (
        StreamHandler<
            ProposalPart,
            TestStreamId,
            Receiver<ReceivedBroadcastedMessage<StreamMessage>>,
            FakeBroadcastClient,
        >,
        Sender<ReceivedBroadcastedMessage<StreamMessage>>,
        Receiver<Receiver<ProposalPart>>,
        Sender<(TestStreamId, Receiver<ProposalPart>)>,
        Receiver<StreamMessage>,
    ) {
        let (inbound_internal_sender, inbound_internal_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (inbound_network_sender, inbound_network_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (outbound_internal_sender, outbound_internal_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (outbound_network_sender, outbound_network_receiver) = mpsc::channel(CHANNEL_SIZE);
        let outbound_network_sender = FakeBroadcastClient { sender: outbound_network_sender };
        let stream_handler = StreamHandler::new(
            inbound_internal_sender,
            inbound_network_receiver,
            outbound_internal_receiver,
            outbound_network_sender,
        );

        (
            stream_handler,
            inbound_network_sender,
            inbound_internal_receiver,
            outbound_internal_sender,
            outbound_network_receiver,
        )
    }

    fn build_init_message(round: u32, stream_id: u64, message_id: u32) -> StreamMessage {
        StreamMessage {
            message: StreamMessageBody::Content(ProposalPart::Init(ProposalInit {
                round,
                ..Default::default()
            })),
            stream_id: TestStreamId(stream_id),
            message_id: message_id.into(),
        }
    }

    fn build_fin_message(stream_id: u64, message_id: u32) -> StreamMessage {
        StreamMessage {
            message: StreamMessageBody::Fin,
            stream_id: TestStreamId(stream_id),
            message_id: message_id.into(),
        }
    }

    #[tokio::test]
    async fn outbound_single() {
        let num_messages = 5;
        let stream_id = 1;
        let (
            mut stream_handler,
            _inbound_network_sender,
            _inbound_internal_receiver,
            mut client_to_streamhandler_sender,
            mut streamhandler_to_network_receiver,
        ) = setup();

        // Create a new stream to send.
        let (mut sender, stream_receiver) = mpsc::channel(CHANNEL_SIZE);
        client_to_streamhandler_sender
            .send((TestStreamId(stream_id), stream_receiver))
            .await
            .unwrap();
        stream_handler.handle_next_msg().await.unwrap();

        // Send the content of the stream.
        for i in 0..num_messages {
            let init = ProposalPart::Init(ProposalInit { round: i, ..Default::default() });
            sender.send(init).await.unwrap();
        }

        // Check the content is sent to the network in order.
        for i in 0..num_messages {
            stream_handler.handle_next_msg().await.unwrap();
            let actual = streamhandler_to_network_receiver.next().now_or_never().unwrap().unwrap();
            assert_eq!(actual, build_init_message(i, stream_id, i));
        }

        // Close the stream and check that a Fin is sent to the network.
        sender.close_channel();
        stream_handler.handle_next_msg().await.unwrap();
        assert_eq!(
            streamhandler_to_network_receiver.next().now_or_never().unwrap().unwrap(),
            build_fin_message(stream_id, num_messages)
        );
    }

    #[tokio::test]
    async fn outbound_multiple() {
        let num_messages = 5;
        let num_streams = 3;
        let (
            mut stream_handler,
            _inbound_network_sender,
            _inbound_internal_receiver,
            mut client_to_streamhandler_sender,
            mut streamhandler_to_network_receiver,
        ) = setup();

        // Client opens up multiple outbound streams.
        let mut stream_senders = Vec::new();
        for stream_id in 0..num_streams {
            let (sender, stream_receiver) = mpsc::channel(CHANNEL_SIZE);
            stream_senders.push(sender);
            client_to_streamhandler_sender
                .send((TestStreamId(stream_id), stream_receiver))
                .await
                .unwrap();
            stream_handler.handle_next_msg().await.unwrap();
        }

        // Send messages on all of the streams.
        for stream_id in 0..num_streams {
            let sender =
                stream_senders.get_mut(TryInto::<usize>::try_into(stream_id).unwrap()).unwrap();
            for i in 0..num_messages {
                let init = ProposalPart::Init(ProposalInit { round: i, ..Default::default() });
                sender.send(init).await.unwrap();
            }
        }

        // {StreamId : [Msgs]} - asserts order received matches expected order per stream.
        let mut expected_msgs = (0..num_streams).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut actual_msgs = expected_msgs.clone();
        for stream_id in 0..num_streams {
            for i in 0..num_messages {
                // The order the stream handler selects from among multiple streams is undefined.
                stream_handler.handle_next_msg().await.unwrap();
                let msg = streamhandler_to_network_receiver.next().now_or_never().unwrap().unwrap();
                actual_msgs[TryInto::<usize>::try_into(msg.stream_id.0).unwrap()].push(msg);
                expected_msgs[TryInto::<usize>::try_into(stream_id).unwrap()]
                    .push(build_init_message(i, stream_id, i));
            }
        }
        assert_eq!(actual_msgs, expected_msgs);

        // Drop all the senders and check Fins are sent.
        stream_senders.clear();
        let mut stream_ids = (0..num_streams).collect::<BTreeSet<_>>();
        for _ in 0..num_streams {
            stream_handler.handle_next_msg().await.unwrap();
            let fin = streamhandler_to_network_receiver.next().now_or_never().unwrap().unwrap();
            assert_eq!(fin.message, StreamMessageBody::Fin);
            assert_eq!(fin.message_id, u64::from(num_messages));
            assert!(stream_ids.remove(&fin.stream_id.0));
        }
    }

    #[tokio::test]
    async fn inbound_in_order() {
        let num_messages = 10;
        let stream_id = 127;
        let (
            mut stream_handler,
            mut inbound_network_sender,
            mut inbound_internal_receiver,
            _client_to_streamhandler_sender,
            _streamhandler_to_network_receiver,
        ) = setup();
        let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

        // Send all messages in order.
        for i in 0..num_messages {
            if i < num_messages - 1 {
                let message = build_init_message(i, stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            } else {
                let message = build_fin_message(stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            }
            stream_handler.handle_next_msg().await.unwrap();
        }

        let mut receiver = inbound_internal_receiver.next().now_or_never().unwrap().unwrap();
        for i in 0..num_messages - 1 {
            // Last message is Fin, so it will not be sent (that's why num_messages - 1)
            let message = receiver.next().await.unwrap();
            assert_eq!(
                message,
                ProposalPart::Init(ProposalInit { round: i, ..Default::default() })
            );
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn inbound_multiple() {
        let num_messages = 5;
        let num_streams = 3;
        let (
            mut stream_handler,
            mut inbound_network_sender,
            mut inbound_internal_receiver,
            _client_to_streamhandler_sender,
            _streamhandler_to_network_receiver,
        ) = setup();
        let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

        // Send all messages to all streams, each stream's messages in order.
        for sid in 0..num_streams {
            for i in 0..num_messages {
                if i < num_messages - 1 {
                    let message = build_init_message(i, sid, i);
                    inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
                } else {
                    let message = build_fin_message(sid, i);
                    inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
                }
                stream_handler.handle_next_msg().await.unwrap();
            }
        }

        let mut expected_msgs = (0..num_streams).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut actual_msgs = expected_msgs.clone();
        for sid in 0..num_streams {
            let mut receiver = inbound_internal_receiver.next().now_or_never().unwrap().unwrap();
            for i in 0..num_messages - 1 {
                // Last message is Fin, so it will not be sent (that's why num_messages - 1).
                let message = receiver.next().await.unwrap();
                actual_msgs
                    .get_mut(TryInto::<usize>::try_into(sid).unwrap())
                    .unwrap()
                    .push(message);
                expected_msgs
                    .get_mut(TryInto::<usize>::try_into(sid).unwrap())
                    .unwrap()
                    .push(ProposalPart::Init(ProposalInit { round: i, ..Default::default() }));
            }
            // Check that the receiver was closed:
            assert!(matches!(receiver.try_next(), Ok(None)));
        }
        assert_eq!(actual_msgs, expected_msgs);
    }

    #[tokio::test]
    async fn inbound_delayed_first() {
        let num_messages = 10;
        let stream_id = 127;
        let (
            mut stream_handler,
            mut inbound_network_sender,
            mut inbound_internal_receiver,
            _client_to_streamhandler_sender,
            _streamhandler_to_network_receiver,
        ) = setup();
        let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

        // Send all messages besides first one.
        for i in 1..num_messages {
            if i < num_messages - 1 {
                let message = build_init_message(i, stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            } else {
                let message = build_fin_message(stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            }
            stream_handler.handle_next_msg().await.unwrap();
        }

        // Check that no receiver was created yet.
        assert!(inbound_internal_receiver.try_next().is_err());

        // Send first message now.
        let first_message = build_init_message(0, stream_id, 0);
        inbound_network_sender.send((Ok(first_message), metadata.clone())).await.unwrap();
        // Activate the stream handler to ingest this message.
        stream_handler.handle_next_msg().await.unwrap();

        // Now first message and all cached messages should be received.
        let mut receiver = inbound_internal_receiver.next().now_or_never().unwrap().unwrap();
        for i in 0..num_messages - 1 {
            // Last message is Fin, so it will not be sent (that's why num_messages - 1)
            let message = receiver.next().await.unwrap();
            assert_eq!(
                message,
                ProposalPart::Init(ProposalInit { round: i, ..Default::default() })
            );
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn inbound_delayed_middle() {
        let num_messages = 10;
        let missing_message_id = 3;
        let stream_id = 127;
        let (
            mut stream_handler,
            mut inbound_network_sender,
            mut inbound_internal_receiver,
            _client_to_streamhandler_sender,
            _streamhandler_to_network_receiver,
        ) = setup();
        let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

        // Send all messages besides one in the middle of the stream.
        for i in 0..num_messages {
            if i == missing_message_id {
                continue;
            }
            if i < num_messages - 1 {
                let message = build_init_message(i, stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            } else {
                let message = build_fin_message(stream_id, i);
                inbound_network_sender.send((Ok(message), metadata.clone())).await.unwrap();
            }
            stream_handler.handle_next_msg().await.unwrap();
        }

        // Should receive a few messages, until we reach the missing one.
        let mut receiver = inbound_internal_receiver.next().now_or_never().unwrap().unwrap();
        for i in 0..missing_message_id {
            let message = receiver.next().await.unwrap();
            assert_eq!(
                message,
                ProposalPart::Init(ProposalInit { round: i, ..Default::default() })
            );
        }

        // Send the missing message now.
        let missing_msg = build_init_message(missing_message_id, stream_id, missing_message_id);
        inbound_network_sender.send((Ok(missing_msg), metadata.clone())).await.unwrap();
        // Activate the stream handler to ingest this message.
        stream_handler.handle_next_msg().await.unwrap();

        // Should now get missing message and all the following ones.
        for i in missing_message_id..num_messages - 1 {
            // Last message is Fin, so it will not be sent (that's why num_messages - 1)
            let message = receiver.next().await.unwrap();
            assert_eq!(
                message,
                ProposalPart::Init(ProposalInit { round: i, ..Default::default() })
            );
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }
}
