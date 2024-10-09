//! Stream handler, see StreamManager struct.
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};

use futures::channel::mpsc;
use futures::StreamExt;
use papyrus_network::network_manager::{
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
};
use papyrus_network::utils::StreamHashMap;
use papyrus_network_types::network_types::{BroadcastedMessageManager, OpaquePeerId};
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

fn get_metadata_peer_id(metadata: &BroadcastedMessageManager) -> PeerId {
    metadata.originator_id.clone()
}

#[derive(Debug, Clone)]
struct StreamData<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> {
    peer_id: PeerId,

    next_message_id: MessageId,
    // The message_id of the message that is marked as "fin" (the last message),
    // if None, it means we have not yet gotten to it.
    fin_message_id: Option<MessageId>,
    max_message_id: MessageId,

    // The sender that corresponds to the receiver that was sent out for this stream.
    sender: mpsc::Sender<T>,

    // A buffer for messages that were received out of order.
    message_buffer: BTreeMap<MessageId, StreamMessage<T>>,
}

impl<T: Clone + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> StreamData<T> {
    fn new(peer_id: PeerId, sender: mpsc::Sender<T>) -> Self {
        StreamData {
            peer_id,
            next_message_id: 0,
            fin_message_id: None,
            max_message_id: 0,
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
    listen_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
    // An end of a channel used to receive messages.
    listen_receiver: BroadcastTopicServer<StreamMessage<T>>,

    // A network sender that allows sending StreamMessages to peers.
    broadcast_sender: BroadcastTopicClient<StreamMessage<T>>,

    // A receiver of receivers, one for each stream_id.
    broadcast_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<T>)>,

    // A map from stream_id to a struct that contains all the information about the stream.
    // This includes both the message buffer and some metadata (like the latest message_id).
    listen_stream_data: HashMap<StreamKey, StreamData<T>>,

    // For each stream that goes out to broadcast, there is a receiver.
    broadcast_stream_receivers: StreamHashMap<StreamId, mpsc::Receiver<T>>,
    broadcast_stream_number: HashMap<StreamId, MessageId>,
}

impl<T: Clone + Send + Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    StreamHandler<T>
{
    /// Create a new StreamHandler.
    pub fn new(
        listen_channel_sender: mpsc::Sender<mpsc::Receiver<T>>,
        listen_receiver: BroadcastTopicServer<StreamMessage<T>>,
        broadcast_sender: BroadcastTopicClient<StreamMessage<T>>,
        broadcast_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<T>)>,
    ) -> Self {
        StreamHandler {
            listen_channel_sender,
            listen_receiver,
            broadcast_sender,
            broadcast_channel_receiver,
            listen_stream_data: HashMap::new(),
            broadcast_stream_receivers: StreamHashMap::new(HashMap::new()),
            broadcast_stream_number: HashMap::new(),
        }
    }

    /// Listen for messages on the receiver channel, buffering them if necessary.
    /// Guarantees that messages are sent in order.
    pub async fn run(&mut self) {
        loop {
            // The StreamHashMap doesn't report back that some of the channels were closed,
            // but the relevant keys are removed when that happens. So check before-after:
            let before: HashSet<_> = self.broadcast_stream_receivers.keys().cloned().collect();

            // Go over the broadcast_channel_receiver to see if there is a new receiver,
            // and go over all existing broadcast_receivers to see if there are any messages to
            // send. Finally, check if there is an input message from the network.
            tokio::select!(
                Some((stream_id, receiver)) = self.broadcast_channel_receiver.next() => {
                    self.broadcast_stream_receivers.insert(stream_id, receiver);
                }
                Some((key, message)) = self.broadcast_stream_receivers.next() => {
                    println!("Got message! ");
                    self.broadcast(key, message).await;
                }
                Some(message) = self.listen_receiver.next() => {
                    self.handle_message(message);
                }
            );
            let after: HashSet<_> = self.broadcast_stream_receivers.keys().cloned().collect();

            println!("before: {:?} | after: {:?}", before, after);
            let diff = before.difference(&after).collect::<HashSet<_>>();
            for key in diff {
                println!("Removing key: {:?}", key);
                self.broadcast_fin(*key).await;
            }
        }
    }

    fn internal_send(data: &mut StreamData<T>, message: StreamMessage<T>) {
        // TODO(guyn): reconsider the "expect" here.
        let sender = &mut data.sender;
        if let StreamMessageBody::Content(content) = message.message {
            sender.try_send(content).expect("Send should succeed");
            data.next_message_id += 1;
        }
    }

    // Send the message to the network.
    async fn broadcast(self: &mut Self, key: StreamId, message: T) {
        let message = StreamMessage {
            message: StreamMessageBody::Content(message),
            stream_id: key,
            message_id: *self.broadcast_stream_number.get(&key).unwrap_or(&0),
        };
        // TODO(guyn): reconsider the "expect" here.
        self.broadcast_sender.broadcast_message(message).await.expect("Send should succeed");
        self.broadcast_stream_number
            .insert(key, self.broadcast_stream_number.get(&key).unwrap_or(&0) + 1);
    }

    // Send a fin message to the network.
    async fn broadcast_fin(self: &mut Self, key: StreamId) {
        let message = StreamMessage {
            message: StreamMessageBody::Fin,
            stream_id: key,
            message_id: *self.broadcast_stream_number.get(&key).unwrap_or(&0),
        };
        self.broadcast_sender.broadcast_message(message).await.expect("Send should succeed");
        self.broadcast_stream_number.remove(&key);
    }

    #[instrument(skip_all, level = "warn")]
    fn handle_message(
        &mut self,
        message: (Result<StreamMessage<T>, ProtobufConversionError>, BroadcastedMessageManager),
    ) {
        let (message, metadata) = message;
        let message = match message {
            Ok(message) => message,
            Err(e) => {
                warn!("Error converting message: {:?}", e);
                return;
            }
        };
        let peer_id = get_metadata_peer_id(&metadata);
        let stream_id = message.stream_id;
        let key = (peer_id.clone(), stream_id);
        let message_id = message.message_id;

        let data = match self.listen_stream_data.entry(key.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(e) => {
                // If we received a message for a stream that we have not seen before,
                // we need to create a new receiver for it.
                let (sender, receiver) = mpsc::channel(CHANNEL_BUFFER_LENGTH);
                // TODO(guyn): reconsider the "expect" here.
                self.listen_channel_sender.try_send(receiver).expect("Send should succeed");

                let data = StreamData::new(peer_id.clone(), sender);
                e.insert(data)
            }
        };

        if data.max_message_id < message_id {
            data.max_message_id = message_id;
        }

        // Check for Fin type message
        match message.message {
            StreamMessageBody::Content(_) => {}
            StreamMessageBody::Fin => {
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
        }

        if message_id > data.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is bigger than the id of the fin message! peer_id: \
                 {:?}, stream_id: {}, message_id: {}, fin_message_id: {}",
                peer_id.clone(),
                stream_id,
                message_id,
                data.fin_message_id.unwrap_or(u64::MAX)
            );
            return;
        }

        // This means we can just send the message without buffering it.
        if message_id == data.next_message_id {
            Self::internal_send(data, message);

            Self::process_buffer(data);

            if data.message_buffer.is_empty() && data.fin_message_id.is_some() {
                data.sender.close_channel();
                self.listen_stream_data.remove(&key);
            }
        } else if message_id > data.next_message_id {
            Self::store(data, message);
        } else {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Received message with id that is smaller than the next message expected! \
                 peer_id: {:?}, stream_id: {}, message_id: {}, next_message_id: {}",
                peer_id, stream_id, message_id, data.next_message_id
            );
            return;
        }
    }

    fn store(data: &mut StreamData<T>, message: StreamMessage<T>) {
        let stream_id = message.stream_id;
        let message_id = message.message_id;

        if data.message_buffer.contains_key(&message_id) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(
                "Two messages with the same message_id in buffer! peer_id: {:?}, stream_id: {}, \
                 message_id: {}",
                data.peer_id.clone(),
                stream_id,
                message_id
            );
        } else {
            data.message_buffer.insert(message_id, message);
        }
    }

    // Tries to drain as many messages as possible from the buffer (in order),
    // DOES NOT guarantee that the buffer will be empty after calling this function.
    fn process_buffer(data: &mut StreamData<T>) {
        while let Some(message) = data.message_buffer.remove(&data.next_message_id) {
            Self::internal_send(data, message);
        }
    }
}
