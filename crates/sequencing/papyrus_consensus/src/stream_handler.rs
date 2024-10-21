//! Stream handler, see StreamManager struct.
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

use futures::channel::mpsc;
use futures::StreamExt;
use papyrus_network::network_manager::BroadcastTopicServer;
use papyrus_network_types::network_types::{BroadcastedMessageMetadata, OpaquePeerId};
use papyrus_protobuf::consensus::{StreamMessage, StreamMessageBody};
use papyrus_protobuf::converters::ProtobufConversionError;
use tracing::{instrument, warn};

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

type PeerId = OpaquePeerId;
type MessageId = u64;
type StreamKey = (PeerId, u64);

const CHANNEL_BUFFER_LENGTH: usize = 100;

#[derive(Debug, Clone)]
struct StreamData<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    next_message_id: MessageId,
    // The message_id of the message that is marked as "fin" (the last message),
    // if None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    max_message_id_received: MessageId,
    // The sender that corresponds to the receiver that was sent out for this stream.
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

/// A StreamHandler is responsible for buffering and sending messages in order.
pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    // An end of a channel used to send out receivers, one for each stream.
    inbound_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
    // An end of a channel used to receive messages.
    inbound_receiver: BroadcastTopicServer<StreamMessage<T>>,
    // A map from stream_id to a struct that contains all the information about the stream.
    // This includes both the message buffer and some metadata (like the latest message_id).
    inbound_stream_data: HashMap<StreamKey, StreamData<T>>,
    // TODO(guyn): perhaps make input_stream_data and output_stream_data?
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    /// Create a new StreamHandler.
    pub fn new(
        inbound_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
        inbound_receiver: BroadcastTopicServer<StreamMessage<T>>,
    ) -> Self {
        StreamHandler {
            inbound_channel_sender,
            inbound_receiver,
            inbound_stream_data: HashMap::new(),
        }
    }

    /// Listen for messages on the receiver channel, buffering them if necessary.
    /// Guarantees that messages are sent in order.
    pub async fn run(&mut self) {
        loop {
            // TODO(guyn): this select is here to allow us to add the outbound flow.
            tokio::select!(
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
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(e) => {
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

        // Check for Fin type message
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

    fn store(data: &mut StreamData<T>, key: StreamKey, message: StreamMessage<T>) {
        let message_id = message.message_id;

        if let std::collections::btree_map::Entry::Vacant(e) = data.message_buffer.entry(message_id)
        {
            e.insert(message);
        } else {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Two messages with the same message_id in buffer! key: {:?}, message_id: {}",
                key, message_id
            );
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
