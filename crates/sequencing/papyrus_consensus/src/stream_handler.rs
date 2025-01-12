//! Stream handler, see StreamManager struct.

use std::cmp::Ordering;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;

use futures::channel::mpsc;
use futures::StreamExt;
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
};
use papyrus_network::utils::StreamHashMap;
use papyrus_network_types::network_types::{BroadcastedMessageMetadata, OpaquePeerId};
use papyrus_protobuf::consensus::{StreamMessage, StreamMessageBody};
use papyrus_protobuf::converters::ProtobufConversionError;
use tracing::{instrument, warn};

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

type PeerId = OpaquePeerId;
type MessageId = u64;

const CHANNEL_BUFFER_LENGTH: usize = 100;

/// A combination of trait bounds needed for the content of the stream.
pub trait StreamContentTrait:
    Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> + Send
{
}
impl<StreamContent> StreamContentTrait for StreamContent where
    StreamContent: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> + Send
{
}
/// A combination of trait bounds needed for the stream ID.
pub trait StreamIdTrait:
    Into<Vec<u8>>
    + TryFrom<Vec<u8>, Error = ProtobufConversionError>
    + Eq
    + Hash
    + Clone
    + Unpin
    + Display
    + Debug
    + Send
{
}
impl<StreamId> StreamIdTrait for StreamId where
    StreamId: Into<Vec<u8>>
        + TryFrom<Vec<u8>, Error = ProtobufConversionError>
        + Eq
        + Hash
        + Clone
        + Unpin
        + Display
        + Debug
        + Send
{
}

// Use this struct for each inbound stream.
// Drop the struct when:
// (1) receiver on the other end is dropped,
// (2) fin message is received and all messages are sent.
#[derive(Debug)]
struct StreamData<StreamContent: StreamContentTrait, StreamId: StreamIdTrait> {
    next_message_id: MessageId,
    // Last message ID. If None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    max_message_id_received: MessageId,
    // Keep the receiver until it is time to send it to the application.
    receiver: Option<mpsc::Receiver<StreamContent>>,
    sender: mpsc::Sender<StreamContent>,
    // A buffer for messages that were received out of order.
    message_buffer: HashMap<MessageId, StreamMessage<StreamContent, StreamId>>,
}

impl<StreamContent: StreamContentTrait, StreamId: StreamIdTrait>
    StreamData<StreamContent, StreamId>
{
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel(CHANNEL_BUFFER_LENGTH);
        StreamData {
            next_message_id: 0,
            fin_message_id: None,
            max_message_id_received: 0,
            sender,
            receiver: Some(receiver),
            message_buffer: HashMap::new(),
        }
    }
}

/// A StreamHandler is responsible for:
/// - Buffering inbound messages and reporting them to the application in order.
/// - Sending outbound messages to the network, wrapped in StreamMessage.
pub struct StreamHandler<StreamContent: StreamContentTrait, StreamId: StreamIdTrait> {
    // For each stream ID from the network, send the application a Receiver
    // that will receive the messages in order. This allows sending such Receivers.
    inbound_channel_sender: mpsc::Sender<mpsc::Receiver<StreamContent>>,
    // This receives messages from the network.
    inbound_receiver: BroadcastTopicServer<StreamMessage<StreamContent, StreamId>>,
    // A map from (peer_id, stream_id) to a struct that contains all the information
    // about the stream. This includes both the message buffer and some metadata
    // (like the latest message ID).
    inbound_stream_data: HashMap<(PeerId, StreamId), StreamData<StreamContent, StreamId>>,
    // Whenever application wants to start a new stream, it must send out a
    // (stream_id, Receiver) pair. Each receiver gets messages that should
    // be sent out to the network.
    outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<StreamContent>)>,
    // A map where the abovementioned Receivers are stored.
    outbound_stream_receivers: StreamHashMap<StreamId, mpsc::Receiver<StreamContent>>,
    // A network sender that allows sending StreamMessages to peers.
    outbound_sender: BroadcastTopicClient<StreamMessage<StreamContent, StreamId>>,
    // For each stream, keep track of the message_id of the last message sent.
    outbound_stream_number: HashMap<StreamId, MessageId>,
}

impl<StreamContent: StreamContentTrait, StreamId: StreamIdTrait>
    StreamHandler<StreamContent, StreamId>
{
    /// Create a new StreamHandler.
    pub fn new(
        inbound_channel_sender: mpsc::Sender<mpsc::Receiver<StreamContent>>,
        inbound_receiver: BroadcastTopicServer<StreamMessage<StreamContent, StreamId>>,
        outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<StreamContent>)>,
        outbound_sender: BroadcastTopicClient<StreamMessage<StreamContent, StreamId>>,
    ) -> Self {
        Self {
            inbound_channel_sender,
            inbound_receiver,
            inbound_stream_data: HashMap::new(),
            outbound_channel_receiver,
            outbound_sender,
            outbound_stream_receivers: StreamHashMap::new(HashMap::new()),
            outbound_stream_number: HashMap::new(),
        }
    }

    /// Create a new StreamHandler and start it running in a new task.
    /// Gets network input/output channels and returns application input/output channels.
    #[allow(clippy::type_complexity)]
    pub fn get_channels(
        inbound_network_receiver: BroadcastTopicServer<StreamMessage<StreamContent, StreamId>>,
        outbound_network_sender: BroadcastTopicClient<StreamMessage<StreamContent, StreamId>>,
    ) -> (
        mpsc::Sender<(StreamId, mpsc::Receiver<StreamContent>)>,
        mpsc::Receiver<mpsc::Receiver<StreamContent>>,
        tokio::task::JoinHandle<()>,
    )
    where
        StreamContent: 'static,
        StreamId: 'static,
    {
        // The inbound messages come into StreamHandler via inbound_network_receiver.
        // The application gets the messages from inbound_internal_receiver
        // (the StreamHandler keeps the inbound_internal_sender to pass the messages).
        let (inbound_internal_sender, inbound_internal_receiver) =
            mpsc::channel(CHANNEL_BUFFER_LENGTH);
        // The outbound messages that an application would like to send are:
        //  1. Sent into outbound_internal_sender as tuples of (StreamId, Receiver)
        //  2. Ingested by StreamHandler by its outbound_internal_receiver.
        //  3. Broadcast by the StreamHandler using its outbound_network_sender.
        let (outbound_internal_sender, outbound_internal_receiver) =
            mpsc::channel(CHANNEL_BUFFER_LENGTH);

        let mut stream_handler = StreamHandler::<StreamContent, StreamId>::new(
            inbound_internal_sender,    // Sender<Receiver<T>>,
            inbound_network_receiver,   // BroadcastTopicServer<StreamMessage<T>>,
            outbound_internal_receiver, // Receiver<(StreamId, Receiver<T>)>,
            outbound_network_sender,    // BroadcastTopicClient<StreamMessage<T>>
        );
        let handle = tokio::spawn(async move {
            stream_handler.run().await;
        });

        (outbound_internal_sender, inbound_internal_receiver, handle)
    }

    /// Listen for messages coming from the network and from the application.
    /// - Outbound messages are wrapped as StreamMessage and sent to the network directly.
    /// - Inbound messages are stripped of StreamMessage and buffered until they can be sent in the
    ///   correct order to the application.
    #[instrument(skip_all)]
    pub async fn run(&mut self) {
        loop {
            tokio::select!(
                // Go over the channel receiver to see if there is a new channel.
                Some((stream_id, receiver)) = self.outbound_channel_receiver.next() => {
                    self.outbound_stream_receivers.insert(stream_id, receiver);
                }
                // Go over all existing outbound receivers to see if there are any messages.
                output = self.outbound_stream_receivers.next() => {
                    match output {
                        Some((key, Some(message))) => {
                            self.broadcast(key, message).await;
                        }
                        Some((key, None)) => {
                            self.broadcast_fin(key).await;
                        }
                        None => {
                            warn!(
                                "StreamHashMap should not be closed! \
                                 Usually only the individual channels are closed. "
                            )
                        }
                    }
                }
                // Check if there is an inbound message from the network.
                Some(message) = self.inbound_receiver.next() => {
                    self.handle_message(message);
                }
            );
        }
    }

    fn inbound_send(
        &mut self,
        data: &mut StreamData<StreamContent, StreamId>,
        message: StreamMessage<StreamContent, StreamId>,
    ) -> bool {
        // TODO(guyn): reconsider the "expect" here.
        let sender = &mut data.sender;
        if let StreamMessageBody::Content(content) = message.message {
            match sender.try_send(content) {
                Ok(_) => {}
                Err(e) => {
                    if e.is_disconnected() {
                        warn!(
                            "Sender is disconnected, dropping the message. StreamId: {}, \
                             MessageId: {}",
                            message.stream_id, message.message_id
                        );
                        return true;
                    } else if e.is_full() {
                        // TODO(guyn): replace panic with buffering of the message.
                        panic!(
                            "Sender is full, dropping the message. StreamId: {}, MessageId: {}",
                            message.stream_id, message.message_id
                        );
                    } else {
                        // TODO(guyn): replace panic with more graceful error handling
                        panic!("Unexpected error: {:?}", e);
                    }
                }
            };
            // Send the receiver only once the first message has been sent.
            if message.message_id == 0 {
                // TODO(guyn): consider the expect in both cases.
                // If this is the first message, send the receiver to the application.
                let receiver = data.receiver.take().expect("Receiver should exist");
                // Send the receiver to the application.
                self.inbound_channel_sender.try_send(receiver).expect("Send should succeed");
            }
            data.next_message_id += 1;
            return false;
        }
        // A Fin message is not sent. This is a no-op, can safely return true.
        true
    }

    // Send the message to the network.
    async fn broadcast(&mut self, stream_id: StreamId, message: StreamContent) {
        // TODO(guyn): add a random nonce to the outbound stream ID,
        // such that even if the client sends the same stream ID,
        // (e.g., after a crash) this will be treated as a new stream.
        let message = StreamMessage {
            message: StreamMessageBody::Content(message),
            stream_id: stream_id.clone(),
            message_id: *self.outbound_stream_number.get(&stream_id).unwrap_or(&0),
        };
        // TODO(guyn): reconsider the "expect" here.
        self.outbound_sender.broadcast_message(message).await.expect("Send should succeed");
        self.outbound_stream_number.insert(
            stream_id.clone(),
            self.outbound_stream_number.get(&stream_id).unwrap_or(&0) + 1,
        );
    }

    // Send a fin message to the network.
    async fn broadcast_fin(&mut self, stream_id: StreamId) {
        let message = StreamMessage {
            message: StreamMessageBody::Fin,
            stream_id: stream_id.clone(),
            message_id: *self.outbound_stream_number.get(&stream_id).unwrap_or(&0),
        };
        self.outbound_sender.broadcast_message(message).await.expect("Send should succeed");
        self.outbound_stream_number.remove(&stream_id);
    }

    // Handle a message that was received from the network.
    #[instrument(skip_all, level = "warn")]
    fn handle_message(
        &mut self,
        message: (
            Result<StreamMessage<StreamContent, StreamId>, ProtobufConversionError>,
            BroadcastedMessageMetadata,
        ),
    ) {
        let (message, metadata) = message;
        let message = match message {
            Ok(message) => message,
            Err(e) => {
                warn!("Error converting message: {:?}", e);
                return;
            }
        };

        let peer_id = metadata.originator_id.clone();
        let stream_id = message.stream_id.clone();
        let key = (peer_id, stream_id);

        let data = match self.inbound_stream_data.entry(key.clone()) {
            // If data exists, remove it (it will be returned to hash map at end of function).
            Occupied(entry) => entry.remove_entry().1,
            Vacant(_) => {
                // If we received a message for a stream that we have not seen before,
                // we need to create a new receiver for it.
                StreamData::new()
            }
        };
        if let Some(data) = self.handle_message_inner(message, metadata, data) {
            self.inbound_stream_data.insert(key, data);
        }
    }

    /// Returns the StreamData struct if it should be put back into the hash map. None if the data
    /// should be dropped.
    fn handle_message_inner(
        &mut self,
        message: StreamMessage<StreamContent, StreamId>,
        metadata: BroadcastedMessageMetadata,
        mut data: StreamData<StreamContent, StreamId>,
    ) -> Option<StreamData<StreamContent, StreamId>> {
        let peer_id = metadata.originator_id;
        let stream_id = message.stream_id.clone();
        let key = (peer_id, stream_id);
        let message_id = message.message_id;

        if data.max_message_id_received < message_id {
            data.max_message_id_received = message_id;
        }

        // Check for Fin type message.
        match message.message {
            StreamMessageBody::Content(_) => {}
            StreamMessageBody::Fin => {
                data.fin_message_id = Some(message_id);
                if data.max_message_id_received > message_id {
                    // TODO(guyn): replace warnings with more graceful error handling
                    warn!(
                        "Received fin message with id that is smaller than a previous message! \
                         key: {:?}, fin_message_id: {}, max_message_id_received: {}",
                        key.clone(),
                        message_id,
                        data.max_message_id_received
                    );
                    return None;
                }
            }
        }

        if message_id > data.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is bigger than the id of the fin message! key: \
                 {:?}, message_id: {}, fin_message_id: {}",
                key.clone(),
                message_id,
                data.fin_message_id.unwrap_or(u64::MAX)
            );
            return None;
        }

        // This means we can just send the message without buffering it.
        match message_id.cmp(&data.next_message_id) {
            Ordering::Equal => {
                let mut receiver_dropped = self.inbound_send(&mut data, message);
                if !receiver_dropped {
                    receiver_dropped = self.process_buffer(&mut data);
                }

                if data.message_buffer.is_empty() && data.fin_message_id.is_some()
                    || receiver_dropped
                {
                    data.sender.close_channel();
                    return None;
                }
            }
            Ordering::Greater => {
                Self::store(&mut data, key.clone(), message);
            }
            Ordering::Less => {
                // TODO(guyn): replace warnings with more graceful error handling
                warn!(
                    "Received message with id that is smaller than the next message expected! \
                     key: {:?}, message_id: {}, next_message_id: {}",
                    key.clone(),
                    message_id,
                    data.next_message_id
                );
                return None;
            }
        }
        Some(data)
    }

    // Store an inbound message in the buffer.
    fn store(
        data: &mut StreamData<StreamContent, StreamId>,
        key: (PeerId, StreamId),
        message: StreamMessage<StreamContent, StreamId>,
    ) {
        let message_id = message.message_id;

        match data.message_buffer.entry(message_id) {
            Vacant(e) => {
                e.insert(message);
            }
            Occupied(_) => {
                // TODO(guyn): replace warnings with more graceful error handling
                warn!(
                    "Two messages with the same message_id in buffer! key: {:?}, message_id: {}",
                    key, message_id
                );
            }
        }
    }

    // Tries to drain as many messages as possible from the buffer (in order),
    // DOES NOT guarantee that the buffer will be empty after calling this function.
    // Returns true if the receiver for this stream is dropped.
    fn process_buffer(&mut self, data: &mut StreamData<StreamContent, StreamId>) -> bool {
        while let Some(message) = data.message_buffer.remove(&data.next_message_id) {
            if self.inbound_send(data, message) {
                return true;
            }
        }
        false
    }
}
