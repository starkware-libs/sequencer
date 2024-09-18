//! Stream handler, see StreamManager struct.
use std::collections::BTreeMap;

use futures::channel::mpsc;
use papyrus_protobuf::consensus::StreamMessage;
use papyrus_protobuf::converters::ProtobufConversionError;

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

/// Configuration for the StreamHandler.
#[derive(Default)]
pub struct StreamHandlerConfig {
    /// The maximum buffer size for each stream (None -> no limit).
    pub max_buffer_size: Option<u64>,
    /// The maximum number of streams that can be buffered at the same time (None -> no limit).
    pub max_num_streams: Option<u64>,
}

type StreamId = u64;
type MessageId = u64;

#[derive(Default)]
struct StreamStats {
    // the next message_id that is expected
    next_message_id: MessageId,
    // the message_id of the message that is marked as "fin" (the last message)
    // if None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    // the highest message_id that was received
    max_message_id: MessageId,
    // the number of messages that are currently buffered
    num_buffered: u64,
}

/// A StreamHandler is responsible for buffering and sending messages in order.
pub struct StreamHandler<
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    /// Configuration for the StreamHandler (things like max buffer size, etc.).
    pub config: StreamHandlerConfig,

    /// An end of a channel used to send out the messages in the correct order.
    pub sender: mpsc::Sender<StreamMessage<T>>,
    /// An end of a channel used to receive messages.
    pub receiver: mpsc::Receiver<StreamMessage<T>>,

    // some statistics about each stream
    stats_per_stream: BTreeMap<StreamId, StreamStats>,

    /// A separate message buffer for each stream_id. For each stream_id there's a nested BTreeMap.
    /// Each nested map is keyed by the message_id of the first message it contains.
    /// The messages in each nested map are stored in a contiguous sequence of messages (as a Vec).
    message_buffers: BTreeMap<StreamId, BTreeMap<MessageId, Vec<StreamMessage<T>>>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    /// Create a new StreamHandler.
    pub fn new(
        config: StreamHandlerConfig,
        sender: mpsc::Sender<StreamMessage<T>>,
        receiver: mpsc::Receiver<StreamMessage<T>>,
    ) -> Self {
        StreamHandler {
            config,
            sender,
            receiver,
            stats_per_stream: BTreeMap::new(),
            message_buffers: BTreeMap::new(),
        }
    }

    /// Listen for messages on the receiver channel, buffering them if necessary.
    /// Guarntees that messages are sent in order.
    pub async fn listen(&mut self) {
        loop {
            if let Ok(message) = self.receiver.try_next() {
                if !self.handle_message(message) {
                    break;
                }
            } else {
                // Err comes when the channel is open but no message was received.
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }
    }

    #[cfg(test)]
    pub async fn listen_with_timeout(&mut self, timeout_millis: u128) {
        let t0 = std::time::Instant::now();
        loop {
            if t0.elapsed().as_millis() > timeout_millis {
                break;
            }
            if let Ok(message) = self.receiver.try_next() {
                if !self.handle_message(message) {
                    break;
                }
            } else {
                // Err comes when the channel is open but no message was received.
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }
    }

    // Handle the message, return true if the channel is still open.
    fn handle_message(&mut self, message: Option<StreamMessage<T>>) -> bool {
        let message = match message {
            Some(message) => message,
            None => {
                // Message is none in case the channel was closed!
                return false;
            }
        };
        let stream_id = message.stream_id;
        let message_id = message.message_id;
        if !self.stats_per_stream.contains_key(&stream_id) {
            self.stats_per_stream.insert(stream_id, StreamStats::default());
        }

        // Check if there are too many streams:
        if let Some(max_streams) = self.config.max_num_streams {
            let num_streams = self.stats_per_stream.len() as u64;
            if num_streams > max_streams {
                panic!("Max number of streams reached! {}", max_streams);
            }
        }

        let stats = self.stats_per_stream.get_mut(&stream_id).unwrap();

        if stats.max_message_id < message_id {
            stats.max_message_id = message_id;
        }

        if message.fin {
            stats.fin_message_id = Some(message_id);
            if stats.max_message_id > message_id {
                panic!(
                    "Received fin message with message_id {} that is smaller than the \
                     max_message_id {}",
                    message_id, stats.max_message_id
                );
            }
        }

        // Check that message_id is not bigger than the fin_message_id.
        if message_id > stats.fin_message_id.unwrap_or(u64::MAX) {
            panic!(
                "Received message with message_id {} that is bigger than the fin_message_id {}",
                message_id,
                stats.fin_message_id.unwrap_or(u64::MAX)
            );
        }

        // This means we can just send the message without buffering it.
        if message_id == stats.next_message_id {
            self.sender.try_send(message).expect("Send should succeed");
            stats.next_message_id += 1;
            // Try to drain the buffer.
            self.drain_buffer(stream_id);
        } else if message_id > stats.next_message_id {
            // Save the message in the buffer.
            self.store(message);
        } else {
            panic!(
                "Received message with message_id {} that is smaller than next_message_id {}",
                message_id, stats.next_message_id
            );
        }

        true // Everything is fine, the channel is still open.
    }

    // Go over each vector in the buffer, push to the end of it if the message_id is contiguous
    // if no vector has a contiguous message_id, start a new vector.
    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;
        let stats = self.stats_per_stream.get_mut(&stream_id).unwrap();
        stats.num_buffered += 1;

        if let Some(max_buf_size) = self.config.max_buffer_size {
            if stats.num_buffered > max_buf_size {
                panic!(
                    "Buffer is full! stream_id= {} with {} messages!",
                    stream_id, stats.num_buffered
                );
            }
        }
        let buffer = self.message_buffers.entry(stream_id).or_insert(BTreeMap::new());
        let keys = buffer.keys().cloned().collect::<Vec<u64>>();
        for id in keys {
            // Go over the keys in order from smallest to largest id.
            let last_id = buffer[&id].last().unwrap().message_id;

            // We can just add the message to the end of the vector.
            if last_id == message_id - 1 {
                buffer.get_mut(&id).unwrap().push(message);
                return;
            }

            // No vector with last message_id will match, skip the rest of the loop.
            if last_id > message_id {
                break;
            }

            // This message should already be inside this vector!
            if message_id >= id || last_id < message_id - 1 {
                let old_message = buffer[&id].iter().filter(|m| m.message_id == message_id).next();
                if let Some(old_message) = old_message {
                    panic!(
                        "Two messages with the same message_id in buffer! Old message: {}, new \
                         message: {}",
                        old_message, message
                    );
                } else if old_message.is_none() {
                    panic!(
                        "Message with message_id {} should be in buffer but is not! ",
                        message_id
                    );
                }
            }
        }
        buffer.insert(message_id, vec![message]);
    }

    // Tries to drain as many messages as possible from the buffer (in order),
    // DOES NOT guarantee that the buffer will be empty after calling this function.
    fn drain_buffer(&mut self, stream_id: u64) {
        let stats = self.stats_per_stream.get_mut(&stream_id).unwrap();
        if let Some(buffer) = self.message_buffers.get_mut(&stream_id) {
            // Drain each vec of messages one by one, if they are in order.
            // To drain a vector, we must match the first id (the key) with the message_id.
            // This while loop will keep draining vectors one by one, until message_id doesn't
            // match.
            while let Some(messages) = buffer.remove(&stats.next_message_id) {
                for message in messages {
                    self.sender.try_send(message).expect("Send should succeed");
                    stats.next_message_id += 1;
                    stats.num_buffered -= 1;
                }
            }

            if buffer.is_empty() && stats.fin_message_id.is_some() {
                self.message_buffers.remove(&stream_id);
                self.stats_per_stream.remove(&stream_id);
            }
        }
    }
}
