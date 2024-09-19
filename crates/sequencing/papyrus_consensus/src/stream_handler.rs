//! Stream handler, see StreamManager struct.
use std::collections::BTreeMap;

use futures::channel::mpsc;
use futures::StreamExt;
use papyrus_network::network_manager::BroadcastClientTrait;
use papyrus_protobuf::consensus::StreamMessage;
use papyrus_protobuf::converters::ProtobufConversionError;

use crate::types::ConsensusError;

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

/// Configuration for the StreamCollector.
#[derive(Default)]
pub struct StreamCollectorConfig {
    /// The maximum buffer size for each stream (None -> no limit).
    pub max_buffer_size: Option<u64>,
    /// The maximum number of streams that can be buffered at the same time (None -> no limit).
    pub max_num_streams: Option<u64>,
}

type StreamId = u64;
type MessageId = u64;

#[derive(Default)]
struct StreamStats {
    // The next message_id that is expected.
    next_message_id: MessageId,
    // The message_id of the message that is marked as "fin" (the last message),
    // if None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    // The highest message_id that was received.
    max_message_id: MessageId,
    // The number of messages that are currently buffered.
    num_buffered: u64,
}

pub struct StreamCollector<
    BroadcastClientT: BroadcastClientTrait<StreamMessage<T>>,
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> {
    /// Configuration for the StreamCollector (things like max buffer size, etc.).
    pub config: StreamCollectorConfig,

    /// A broadcast client (receiver) that gets messages from the network.
    pub receiver: BroadcastClientT,

    /// A channel used to send receivers, one for each stream that was opened.
    /// These channels will be closed once all messages for a stream are transmitted.
    pub sender: mpsc::Sender<mpsc::Receiver<StreamMessage<T>>>,

    sender_per_stream: BTreeMap<StreamId, mpsc::Sender<StreamMessage<T>>>,

    // Some statistics about each stream.
    stats_per_stream: BTreeMap<StreamId, StreamStats>,

    /// A separate message buffer for each stream_id. For each stream_id there's a nested BTreeMap.
    /// Each nested map is keyed by the message_id of the first message it contains.
    /// The messages in each nested map are stored in a contiguous sequence of messages (as a Vec).
    message_buffers: BTreeMap<StreamId, BTreeMap<MessageId, Vec<StreamMessage<T>>>>,

    /// the highest number of stream_id we've used so far
    last_stream_id: StreamId,
}

impl<
    BroadcastClientT: BroadcastClientTrait<StreamMessage<T>>,
    T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>,
> StreamCollector<BroadcastClientT, T>
{
    /// Create a new StreamCollector.
    pub fn new(
        config: StreamCollectorConfig,
        receiver: BroadcastClientT,
        sender: mpsc::Sender<mpsc::Receiver<StreamMessage<T>>>,
    ) -> Self {
        StreamCollector {
            config,
            receiver,
            sender,
            sender_per_stream: BTreeMap::new(),
            stats_per_stream: BTreeMap::new(),
            message_buffers: BTreeMap::new(),
            last_stream_id: 0,
        }
    }

    /// Listen for messages on the receiver channel, buffering them if necessary.
    /// Guarntees that messages are sent in order.
    pub async fn listen(&mut self) {
        loop {
            let (message, broadcasted_message_manager) =
                self.receiver.next().await.ok_or_else(|| {
                    ConsensusError::InternalNetworkError(
                        "NetworkReceiver should never be closed".to_string(),
                    )
                });
            let message = match message {
                Some(message) => message,
                None => {
                    // Message is none in case the channel was closed!
                    break;
                }
            };
            let peer_bytes: () = broadcasted_message_manager.peer_id.to_bytes();
            // self.handle_message(message, peer_bytes);
        }
    }

    // Handle the message, return true if the channel is still open.
    fn handle_message(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;
        if !self.stats_per_stream.contains_key(&stream_id) {
            self.stats_per_stream.insert(stream_id, StreamStats::default());
        }

        // Check if there are too many streams:
        if let Some(max_streams) = self.config.max_num_streams {
            let num_streams = self.stats_per_stream.len() as u64;
            if num_streams > max_streams {
                // TODO(guyn): replace panics with more graceful error handling
                panic!("Maximum number of streams reached! {}", max_streams);
            }
        }

        let stats = self.stats_per_stream.get_mut(&stream_id).unwrap();

        if stats.max_message_id < message_id {
            stats.max_message_id = message_id;
        }

        if message.fin {
            stats.fin_message_id = Some(message_id);
            if stats.max_message_id > message_id {
                // TODO(guyn): replace panics with more graceful error handling
                panic!(
                    "Received fin message with id that is smaller than a previous message! \
                     stream_id: {}, fin_message_id: {}, max_message_id: {}",
                    stream_id, message_id, stats.max_message_id
                );
            }
        }

        // Check that message_id is not bigger than the fin_message_id.
        if message_id > stats.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace panics with more graceful error handling
            panic!(
                "Received message with id that is bigger than the id of the fin message! \
                 stream_id: {}, message_id: {}, fin_message_id: {}",
                stream_id,
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
            // TODO(guyn): replace panics with more graceful error handling
            panic!(
                "Received message with id that is smaller than the next message expected! \
                 stream_id: {}, message_id: {}, next_message_id: {}",
                stream_id, message_id, stats.next_message_id
            );
        }
    }

    // Go over each vector in the buffer, push to the end of it if the message_id is contiguous.
    // If no vector has a contiguous message_id, start a new vector.
    fn store(&mut self, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;
        let stats = self.stats_per_stream.get_mut(&stream_id).unwrap();
        stats.num_buffered += 1;

        if let Some(max_buf_size) = self.config.max_buffer_size {
            if stats.num_buffered > max_buf_size {
                // TODO(guyn): replace panics with more graceful error handling
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
                    // TODO(guyn): replace panics with more graceful error handling
                    panic!(
                        "Two messages with the same message_id in buffer! stream_id: {}, old \
                         message: {}, new message: {}",
                        stream_id, old_message, message
                    );
                } else if old_message.is_none() {
                    // TODO(guyn): replace panics with more graceful error handling
                    panic!(
                        "Message with this id should be in buffer, but is not! 
                        stream_id: {}, message_id: {}",
                        stream_id, message_id
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
