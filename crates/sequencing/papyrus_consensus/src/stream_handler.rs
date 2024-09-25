//! Stream handler, see StreamManager struct.
use std::collections::{BTreeMap, HashMap};

use futures::channel::mpsc;
use futures::StreamExt;
use papyrus_protobuf::consensus::StreamMessage;
use papyrus_protobuf::converters::ProtobufConversionError;
use tracing::{instrument, warn};

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

type StreamId = u64;
type MessageId = u64;

#[derive(Debug, Clone)]
struct StreamData<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    // The next message_id that is expected.
    next_message_id: MessageId,
    // The message_id of the message that is marked as "fin" (the last message),
    // if None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    // The highest message_id that was received.
    max_message_id: MessageId,

    message_buffer: BTreeMap<MessageId, StreamMessage<T>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> Default
    for StreamData<T>
{
    fn default() -> Self {
        StreamData {
            next_message_id: 0,
            fin_message_id: None,
            max_message_id: 0,
            message_buffer: BTreeMap::new(),
        }
    }
}

/// A StreamHandler is responsible for buffering and sending messages in order.
pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    // An end of a channel used to send out the messages in the correct order.
    sender: mpsc::Sender<T>,
    // An end of a channel used to receive messages.
    receiver: mpsc::Receiver<StreamMessage<T>>,

    // A map from stream_id to a struct that contains all the information about the stream.
    // This includes both the message buffer and some metadata (like the latest message_id).
    stream_data: HashMap<StreamId, StreamData<T>>,
    // TODO(guyn): perhaps make input_stream_data and output_stream_data?
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    /// Create a new StreamHandler.
    pub fn new(sender: mpsc::Sender<T>, receiver: mpsc::Receiver<StreamMessage<T>>) -> Self {
        StreamHandler { sender, receiver, stream_data: HashMap::new() }
    }

    /// Listen for messages on the receiver channel, buffering them if necessary.
    /// Guarantees that messages are sent in order.
    pub async fn listen(&mut self) {
        loop {
            if let Some(message) = self.receiver.next().await {
                self.handle_message(message);
            }
        }
    }

    fn send(data: &mut StreamData<T>, sender: &mut mpsc::Sender<T>, message: StreamMessage<T>) {
        // TODO(guyn): reconsider the "expect" here.
        sender.try_send(message.message).expect("Send should succeed");
        data.next_message_id += 1;
    }

    // Handle the message, return true if the channel is still open.
    #[instrument(skip_all, level = "warn")]
    fn handle_message(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;

        let data = self.stream_data.entry(stream_id).or_default();

        if data.max_message_id < message_id {
            data.max_message_id = message_id;
        }

        if message.fin {
            data.fin_message_id = Some(message_id);
            if data.max_message_id > message_id {
                // TODO(guyn): replace warnings with more graceful error handling
                warn!(
                    "Received fin message with id that is smaller than a previous message! \
                     stream_id: {}, fin_message_id: {}, max_message_id: {}",
                    stream_id, message_id, data.max_message_id
                );
                return;
            }
        }

        // Check that message_id is not bigger than the fin_message_id.
        if message_id > data.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is bigger than the id of the fin message! \
                 stream_id: {}, message_id: {}, fin_message_id: {}",
                stream_id,
                message_id,
                data.fin_message_id.unwrap_or(u64::MAX)
            );
            return;
        }

        // This means we can just send the message without buffering it.
        if message_id == data.next_message_id {
            Self::send(data, &mut self.sender, message);

            // Try to drain the buffer.
            self.process_buffer(stream_id);
        } else if message_id > data.next_message_id {
            // Save the message in the buffer.
            self.store(message);
        } else {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is smaller than the next message expected! \
                 stream_id: {}, message_id: {}, next_message_id: {}",
                stream_id, message_id, data.next_message_id
            );
            return;
        }
    }

    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;
        let data = self.stream_data.entry(stream_id).or_default();

        if data.message_buffer.contains_key(&message_id) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Two messages with the same message_id in buffer! stream_id: {}, message_id: {}",
                stream_id, message_id
            );
        } else {
            data.message_buffer.insert(message_id, message);
        }
    }

    // Tries to drain as many messages as possible from the buffer (in order),
    // DOES NOT guarantee that the buffer will be empty after calling this function.
    fn process_buffer(&mut self, stream_id: u64) {
        let data = self.stream_data.entry(stream_id).or_default();
        while let Some(message) = data.message_buffer.remove(&data.next_message_id) {
            Self::send(data, &mut self.sender, message);
        }

        if data.message_buffer.is_empty() && data.fin_message_id.is_some() {
            self.stream_data.remove(&stream_id);
        }
    }
}
