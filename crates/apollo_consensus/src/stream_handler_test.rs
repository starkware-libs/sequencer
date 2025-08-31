use std::collections::BTreeSet;
use std::fmt::Display;

use apollo_consensus_config::StreamHandlerConfig;
use apollo_network::network_manager::{BroadcastTopicClientTrait, ReceivedBroadcastedMessage};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{ProposalInit, ProposalPart, StreamMessageBody};
use apollo_protobuf::converters::ProtobufConversionError;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::channel::mpsc::{self, Receiver, SendError, Sender};
use futures::{FutureExt, SinkExt, StreamExt};
use prost::DecodeError;

use crate::stream_handler::StreamHandler;
const CHANNEL_CAPACITY: usize = 100;
const MAX_STREAMS: usize = 10;

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

type StreamMessage = apollo_protobuf::consensus::StreamMessage<ProposalPart, TestStreamId>;

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
    let (inbound_internal_sender, streamhandler_to_client_receiver) =
        mpsc::channel(CHANNEL_CAPACITY);
    let (network_to_streamhandler_sender, inbound_network_receiver) =
        mpsc::channel(CHANNEL_CAPACITY);
    let (outbound_internal_sender, outbound_internal_receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let (outbound_network_sender, outbound_network_receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let outbound_network_sender = FakeBroadcastClient { sender: outbound_network_sender };
    let config =
        StreamHandlerConfig { channel_buffer_capacity: CHANNEL_CAPACITY, max_streams: MAX_STREAMS };
    let stream_handler = StreamHandler::new(
        config,
        inbound_internal_sender,
        inbound_network_receiver,
        outbound_internal_receiver,
        outbound_network_sender,
    );

    (
        stream_handler,
        network_to_streamhandler_sender,
        streamhandler_to_client_receiver,
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

fn as_usize<T: TryInto<usize>>(t: T) -> usize
where
    <T as TryInto<usize>>::Error: std::fmt::Debug,
{
    t.try_into().unwrap()
}

#[tokio::test]
async fn outbound_single() {
    let num_messages = 5;
    let stream_id = 1;
    let (
        mut stream_handler,
        _network_to_streamhandler_sender,
        _streamhandler_to_client_receiver,
        mut client_to_streamhandler_sender,
        mut streamhandler_to_network_receiver,
    ) = setup();

    // Create a new stream to send.
    let (mut sender, stream_receiver) = mpsc::channel(CHANNEL_CAPACITY);
    client_to_streamhandler_sender.send((TestStreamId(stream_id), stream_receiver)).await.unwrap();
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
        _network_to_streamhandler_sender,
        _streamhandler_to_client_receiver,
        mut client_to_streamhandler_sender,
        mut streamhandler_to_network_receiver,
    ) = setup();

    // Client opens up multiple outbound streams.
    let mut stream_senders = Vec::new();
    for stream_id in 0..num_streams {
        let (sender, stream_receiver) = mpsc::channel(CHANNEL_CAPACITY);
        stream_senders.push(sender);
        client_to_streamhandler_sender
            .send((TestStreamId(stream_id), stream_receiver))
            .await
            .unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }

    // Send messages on all of the streams.
    for stream_id in 0..num_streams {
        let sender = stream_senders.get_mut(as_usize(stream_id)).unwrap();
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
            actual_msgs[as_usize(msg.stream_id.0)].push(msg);
            expected_msgs[as_usize(stream_id)].push(build_init_message(i, stream_id, i));
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
        mut network_to_streamhandler_sender,
        mut streamhandler_to_client_receiver,
        _client_to_streamhandler_sender,
        _streamhandler_to_network_receiver,
    ) = setup();
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

    // Send all messages in order.
    for i in 0..num_messages {
        let message = build_init_message(i, stream_id, i);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }
    let message = build_fin_message(stream_id, num_messages);
    network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
    stream_handler.handle_next_msg().await.unwrap();
    // Fin is communicated by dropping the sender, hence `..num_message` not `..=num_messages`
    let mut receiver = streamhandler_to_client_receiver.next().now_or_never().unwrap().unwrap();
    for i in 0..num_messages {
        let message = receiver.next().await.unwrap();
        assert_eq!(message, ProposalPart::Init(ProposalInit { round: i, ..Default::default() }));
    }
    // Check that the receiver was closed:
    assert!(matches!(receiver.try_next(), Ok(None)));
}

#[tokio::test]
async fn lru_cache_for_inbound_streams() {
    let num_streams = MAX_STREAMS + 1;
    let (
        mut stream_handler,
        mut network_to_streamhandler_sender,
        mut streamhandler_to_client_receiver,
        _client_to_streamhandler_sender,
        _streamhandler_to_network_receiver,
    ) = setup();

    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    for i in 0..num_streams {
        let message = build_fin_message(i.try_into().unwrap(), 1);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }

    for i in (0..num_streams).rev() {
        let message = build_init_message(i.try_into().unwrap(), i.try_into().unwrap(), 0);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }

    for i in (0..num_streams).rev() {
        let mut receiver = streamhandler_to_client_receiver.next().now_or_never().unwrap().unwrap();
        let message = receiver.next().await.unwrap();
        assert_eq!(
            message,
            ProposalPart::Init(ProposalInit { round: i.try_into().unwrap(), ..Default::default() })
        );
        if i == 0 {
            // This stream was reopened, but it should only have one message, and left open.
            assert!(receiver.try_next().is_err());
        } else {
            // The rest of the channels should have successfully received all messages,
            // and closed after receiving the Fin message.
            assert!(matches!(receiver.try_next(), Ok(None)));
        }
    }
}

#[tokio::test]
async fn inbound_multiple() {
    let num_messages = 5;
    let num_streams = 3;
    let (
        mut stream_handler,
        mut network_to_streamhandler_sender,
        mut streamhandler_to_client_receiver,
        _client_to_streamhandler_sender,
        _streamhandler_to_network_receiver,
    ) = setup();
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

    // Send all messages to all streams, each stream's messages in order.
    for sid in 0..num_streams {
        for i in 0..num_messages {
            let message = build_init_message(i, sid, i);
            network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
            stream_handler.handle_next_msg().await.unwrap();
        }
        let message = build_fin_message(sid, num_messages);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }

    let mut expected_msgs = (0..num_streams).map(|_| Vec::new()).collect::<Vec<_>>();
    let mut actual_msgs = expected_msgs.clone();
    for sid in 0..num_streams {
        let mut receiver = streamhandler_to_client_receiver.next().now_or_never().unwrap().unwrap();
        // Fin is communicated by dropping the sender, hence `..num_message` not `..=num_messages`
        for i in 0..num_messages {
            let message = receiver.next().await.unwrap();
            actual_msgs.get_mut(as_usize(sid)).unwrap().push(message);
            expected_msgs
                .get_mut(as_usize(sid))
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
        mut network_to_streamhandler_sender,
        mut streamhandler_to_client_receiver,
        _client_to_streamhandler_sender,
        _streamhandler_to_network_receiver,
    ) = setup();
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

    // Send all messages besides first one.
    for i in 1..num_messages {
        let message = build_init_message(i, stream_id, i);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }
    let message = build_fin_message(stream_id, num_messages);
    network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
    stream_handler.handle_next_msg().await.unwrap();

    // Check that no receiver was created yet.
    assert!(streamhandler_to_client_receiver.try_next().is_err());

    // Send first message now.
    let first_message = build_init_message(0, stream_id, 0);
    network_to_streamhandler_sender.send((Ok(first_message), metadata.clone())).await.unwrap();
    // Activate the stream handler to ingest this message.
    stream_handler.handle_next_msg().await.unwrap();

    // Now first message and all cached messages should be received.
    let mut receiver = streamhandler_to_client_receiver.next().now_or_never().unwrap().unwrap();
    // Fin is communicated by dropping the sender, hence `..num_message` not `..=num_messages`
    for i in 0..num_messages {
        let message = receiver.next().await.unwrap();
        assert_eq!(message, ProposalPart::Init(ProposalInit { round: i, ..Default::default() }));
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
        mut network_to_streamhandler_sender,
        mut streamhandler_to_client_receiver,
        _client_to_streamhandler_sender,
        _streamhandler_to_network_receiver,
    ) = setup();
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());

    // Send all messages besides one in the middle of the stream.
    for i in 0..num_messages {
        if i == missing_message_id {
            continue;
        }
        let message = build_init_message(i, stream_id, i);
        network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
        stream_handler.handle_next_msg().await.unwrap();
    }
    let message = build_fin_message(stream_id, num_messages);
    network_to_streamhandler_sender.send((Ok(message), metadata.clone())).await.unwrap();
    stream_handler.handle_next_msg().await.unwrap();

    // Should receive a few messages, until we reach the missing one.
    let mut receiver = streamhandler_to_client_receiver.next().now_or_never().unwrap().unwrap();
    for i in 0..missing_message_id {
        let message = receiver.next().await.unwrap();
        assert_eq!(message, ProposalPart::Init(ProposalInit { round: i, ..Default::default() }));
    }

    // Send the missing message now.
    let missing_msg = build_init_message(missing_message_id, stream_id, missing_message_id);
    network_to_streamhandler_sender.send((Ok(missing_msg), metadata.clone())).await.unwrap();
    // Activate the stream handler to ingest this message.
    stream_handler.handle_next_msg().await.unwrap();

    // Should now get missing message and all the following ones.
    // Fin is communicated by dropping the sender, hence `..num_message` not `..=num_messages`
    for i in missing_message_id..num_messages {
        let message = receiver.next().await.unwrap();
        assert_eq!(message, ProposalPart::Init(ProposalInit { round: i, ..Default::default() }));
    }
    // Check that the receiver was closed:
    assert!(matches!(receiver.try_next(), Ok(None)));
}
