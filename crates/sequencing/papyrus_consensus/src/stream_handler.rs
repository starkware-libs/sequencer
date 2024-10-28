//! Stream handler, see StreamManager struct.

use std::cmp::Ordering;
use std::collections::btree_map::Entry as BTreeEntry;
use std::collections::hash_map::Entry as HashMapEntry;
use std::collections::{BTreeMap, HashMap};

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
type StreamId = u64;
type MessageId = u64;
type StreamKey = (PeerId, StreamId);

const CHANNEL_BUFFER_LENGTH: usize = 100;

#[derive(Debug, Clone)]
struct StreamData<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> + 'static,
> {
    next_message_id: MessageId,
    // Last message ID. If None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    max_message_id_received: MessageId,
    sender: mpsc::Sender<T>,
    // A buffer for messages that were received out of order.
    message_buffer: BTreeMap<MessageId, StreamMessage<T>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> StreamData<T> {
    fn new(sender: mpsc::Sender<T>) -> Self {
        StreamData {
            next_message_id: 0,
            fin_message_id: None,
            max_message_id_received: 0,
            sender,
            message_buffer: BTreeMap::new(),
        }
    }
}

/// A StreamHandler is responsible for:
/// - Buffering inbound messages and reporting them to the application in order.
/// - Sending outbound messages to the network, wrapped in StreamMessage.
pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> + 'static,
> {
    // For each stream ID from the network, send the application a Receiver
    // that will receive the messages in order. This allows sending such Receivers.
    inbound_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
    // This receives messages from the network.
    inbound_receiver: BroadcastTopicServer<StreamMessage<T>>,
    // A map from (peer_id, stream_id) to a struct that contains all the information
    // about the stream. This includes both the message buffer and some metadata
    // (like the latest message ID).
    inbound_stream_data: HashMap<StreamKey, StreamData<T>>,
    // Whenever application wants to start a new stream, it must send out a
    // (stream_id, Receiver) pair. Each receiver gets messages that should
    // be sent out to the network.
    outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<T>)>,
    // A map where the abovementioned Receivers are stored.
    outbound_stream_receivers: StreamHashMap<StreamId, mpsc::Receiver<T>>,
    // A network sender that allows sending StreamMessages to peers.
    outbound_sender: BroadcastTopicClient<StreamMessage<T>>,
    // For each stream, keep track of the message_id of the last message sent.
    outbound_stream_number: HashMap<StreamId, MessageId>,
}

impl<T: Clone + Send + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    /// Create a new StreamHandler.
    pub fn new(
        inbound_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
        inbound_receiver: BroadcastTopicServer<StreamMessage<T>>,
        outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<T>)>,
        outbound_sender: BroadcastTopicClient<StreamMessage<T>>,
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
    pub fn get_channels(
        inbound_network_receiver: BroadcastTopicServer<StreamMessage<T>>,
        outbound_network_sender: BroadcastTopicClient<StreamMessage<T>>,
    ) -> (mpsc::Sender<(StreamId, mpsc::Receiver<T>)>, mpsc::Receiver<mpsc::Receiver<T>>) {
        // The inbound messages come into StreamHandler via inbound_network_receiver,
        // and are forwarded to the consensus via inbound_internal_receiver
        // (the StreamHandler keeps the inbound_internal_sender to pass messsage).
        let (inbound_internal_sender, inbound_internal_receiver): (
            mpsc::Sender<mpsc::Receiver<T>>,
            mpsc::Receiver<mpsc::Receiver<T>>,
        ) = mpsc::channel(CHANNEL_BUFFER_LENGTH);
        // The outbound messages that an application would like to send are:
        //  1. Sent into outbound_internal_sender as tuples of (StreamId, Receiver)
        //  2. Ingested by StreamHandler by its outbound_internal_receiver.
        //  3. Broadcast by the StreamHandler using its outbound_network_sender.
        let (outbound_internal_sender, outbound_internal_receiver): (
            mpsc::Sender<(StreamId, mpsc::Receiver<T>)>,
            mpsc::Receiver<(StreamId, mpsc::Receiver<T>)>,
        ) = mpsc::channel(CHANNEL_BUFFER_LENGTH);

        let mut stream_handler = StreamHandler::<T>::new(
            inbound_internal_sender,    // Sender<Receiver<T>>,
            inbound_network_receiver,   // BroadcastTopicServer<StreamMessage<T>>,
            outbound_internal_receiver, // Receiver<(StreamId, Receiver<T>)>,
            outbound_network_sender,    // BroadcastTopicClient<StreamMessage<T>>
        );
        tokio::spawn(async move {
            stream_handler.run().await;
        });

        (outbound_internal_sender, inbound_internal_receiver)
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

    fn inbound_send(data: &mut StreamData<T>, message: StreamMessage<T>) {
        // TODO(guyn): reconsider the "expect" here.
        let sender = &mut data.sender;
        if let StreamMessageBody::Content(content) = message.message {
            sender.try_send(content).expect("Send should succeed");
            data.next_message_id += 1;
        }
    }

    // Send the message to the network.
    async fn broadcast(&mut self, stream_id: StreamId, message: T) {
        let message = StreamMessage {
            message: StreamMessageBody::Content(message),
            stream_id,
            message_id: *self.outbound_stream_number.get(&stream_id).unwrap_or(&0),
        };
        // TODO(guyn): reconsider the "expect" here.
        self.outbound_sender.broadcast_message(message).await.expect("Send should succeed");
        self.outbound_stream_number
            .insert(stream_id, self.outbound_stream_number.get(&stream_id).unwrap_or(&0) + 1);
    }

    // Send a fin message to the network.
    async fn broadcast_fin(&mut self, stream_id: StreamId) {
        let message = StreamMessage {
            message: StreamMessageBody::Fin,
            stream_id,
            message_id: *self.outbound_stream_number.get(&stream_id).unwrap_or(&0),
        };
        self.outbound_sender.broadcast_message(message).await.expect("Send should succeed");
        self.outbound_stream_number.remove(&stream_id);
    }

    // Handle a message that was received from the network.
    #[instrument(skip_all, level = "warn")]
    fn handle_message(
        &mut self,
        message: (Result<StreamMessage<T>, ProtobufConversionError>, BroadcastedMessageMetadata),
    ) {
        let (message, metadata) = message;
        let message = match message {
            Ok(message) => message,
            Err(e) => {
                warn!("Error converting message: {:?}", e);
                return;
            }
        };
        let peer_id = metadata.originator_id;
        let stream_id = message.stream_id;
        let key = (peer_id, stream_id);
        let message_id = message.message_id;

        let data = match self.inbound_stream_data.entry(key.clone()) {
            HashMapEntry::Occupied(entry) => entry.into_mut(),
            HashMapEntry::Vacant(e) => {
                // If we received a message for a stream that we have not seen before,
                // we need to create a new receiver for it.
                let (sender, receiver) = mpsc::channel(CHANNEL_BUFFER_LENGTH);
                // TODO(guyn): reconsider the "expect" here.
                self.inbound_channel_sender.try_send(receiver).expect("Send should succeed");

                let data = StreamData::new(sender);
                e.insert(data)
            }
        };

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
                        key, message_id, data.max_message_id_received
                    );
                    return;
                }
            }
        }

        if message_id > data.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is bigger than the id of the fin message! key: \
                 {:?}, message_id: {}, fin_message_id: {}",
                key,
                message_id,
                data.fin_message_id.unwrap_or(u64::MAX)
            );
            return;
        }

        // This means we can just send the message without buffering it.
        match message_id.cmp(&data.next_message_id) {
            Ordering::Equal => {
                Self::inbound_send(data, message);
                Self::process_buffer(data);

                if data.message_buffer.is_empty() && data.fin_message_id.is_some() {
                    data.sender.close_channel();
                    self.inbound_stream_data.remove(&key);
                }
            }
            Ordering::Greater => {
                Self::store(data, key, message);
            }
            Ordering::Less => {
                // TODO(guyn): replace warnings with more graceful error handling
                warn!(
                    "Received message with id that is smaller than the next message expected! \
                     key: {:?}, message_id: {}, next_message_id: {}",
                    key, message_id, data.next_message_id
                );
                return;
            }
        }
    }

    // Store an inbound message in the buffer.
    fn store(data: &mut StreamData<T>, key: StreamKey, message: StreamMessage<T>) {
        let message_id = message.message_id;

        match data.message_buffer.entry(message_id) {
            BTreeEntry::Vacant(e) => {
                e.insert(message);
            }
            BTreeEntry::Occupied(_) => {
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
    fn process_buffer(data: &mut StreamData<T>) {
        while let Some(message) = data.message_buffer.remove(&data.next_message_id) {
            Self::inbound_send(data, message);
        }
    }
}
