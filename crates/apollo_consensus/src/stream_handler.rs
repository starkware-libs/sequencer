//! Overlay streaming logic onto individual messages.

use std::cmp::Ordering;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::num::NonZeroUsize;

use apollo_network::network_manager::{BroadcastTopicClientTrait, ReceivedBroadcastedMessage};
use apollo_network::utils::StreamMap;
use apollo_network_types::network_types::{BroadcastedMessageMetadata, OpaquePeerId};
use apollo_protobuf::consensus::{StreamMessage, StreamMessageBody};
use apollo_protobuf::converters::ProtobufConversionError;
use futures::channel::mpsc;
use futures::never::Never;
use futures::StreamExt;
use lru::LruCache;
use tracing::{info, instrument, warn};

use crate::config::StreamHandlerConfig;
use crate::metrics::{
    CONSENSUS_INBOUND_STREAM_EVICTED,
    CONSENSUS_INBOUND_STREAM_FINISHED,
    CONSENSUS_INBOUND_STREAM_STARTED,
    CONSENSUS_OUTBOUND_STREAM_FINISHED,
    CONSENSUS_OUTBOUND_STREAM_STARTED,
};

#[cfg(test)]
#[path = "stream_handler_test.rs"]
mod stream_handler_test;

type PeerId = OpaquePeerId;
type MessageId = u64;

/// Errors which cause the stream handler to stop functioning.
#[derive(thiserror::Error, PartialEq, Debug)]
pub enum StreamHandlerError {
    /// Client has closed their sender, so no more outbound streams can be sent.
    #[error("Client has closed their sender, so no more outbound streams can be sent.")]
    OutboundChannelClosed,
    /// Network has closed their sender, so no more inbound streams can be sent.
    #[error("Network has closed their sender, so no more inbound streams can be sent.")]
    InboundChannelClosed,
    /// StreamId sent by client for a stream which is in use for an existing stream.
    #[error("StreamId sent by client for a stream which is in use for an existing stream. {0}")]
    StreamIdReused(String),
}

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
    + Ord
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
        + Ord
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
    fn new(channel_buffer_capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(channel_buffer_capacity);
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
pub struct StreamHandler<StreamContent, StreamId, InboundReceiverT, OutboundSenderT>
where
    StreamContent: StreamContentTrait,
    StreamId: StreamIdTrait,
    InboundReceiverT: Unpin
        + StreamExt<Item = ReceivedBroadcastedMessage<StreamMessage<StreamContent, StreamId>>>,
    OutboundSenderT: BroadcastTopicClientTrait<StreamMessage<StreamContent, StreamId>>,
{
    config: StreamHandlerConfig,
    // For each stream ID from the network, send the application a Receiver
    // that will receive the messages in order. This allows sending such Receivers.
    inbound_channel_sender: mpsc::Sender<mpsc::Receiver<StreamContent>>,
    // This receives messages from the network.
    inbound_receiver: InboundReceiverT,
    // An LRU cache mapping (peer_id, stream_id) to a struct that contains all the information
    // about the stream. This includes both the message buffer and some metadata
    // (like the latest message ID).
    inbound_stream_data: LruCache<(PeerId, StreamId), StreamData<StreamContent, StreamId>>,
    // Whenever application wants to start a new stream, it must send out a
    // (stream_id, Receiver) pair. Each receiver gets messages that should
    // be sent out to the network.
    outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<StreamContent>)>,
    // A map where the abovementioned Receivers are stored.
    outbound_stream_receivers: StreamMap<StreamId, mpsc::Receiver<StreamContent>>,
    // A network sender that allows sending StreamMessages to peers.
    outbound_sender: OutboundSenderT,
    // For each stream, keep track of the message_id of the last message sent.
    outbound_stream_number: HashMap<StreamId, MessageId>,
}

impl<StreamContent, StreamId, InboundReceiverT, OutboundSenderT>
    StreamHandler<StreamContent, StreamId, InboundReceiverT, OutboundSenderT>
where
    StreamContent: StreamContentTrait,
    StreamId: StreamIdTrait,
    InboundReceiverT: Unpin
        + StreamExt<Item = ReceivedBroadcastedMessage<StreamMessage<StreamContent, StreamId>>>,
    OutboundSenderT: BroadcastTopicClientTrait<StreamMessage<StreamContent, StreamId>>,
{
    /// Create a new StreamHandler.
    pub fn new(
        config: StreamHandlerConfig,
        inbound_channel_sender: mpsc::Sender<mpsc::Receiver<StreamContent>>,
        inbound_receiver: InboundReceiverT,
        outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<StreamContent>)>,
        outbound_sender: OutboundSenderT,
    ) -> Self {
        let cache = LruCache::new(
            NonZeroUsize::new(config.max_streams).expect("max_streams must be non-zero"),
        );

        Self {
            config,
            inbound_channel_sender,
            inbound_receiver,
            inbound_stream_data: cache,
            outbound_channel_receiver,
            outbound_sender,
            outbound_stream_receivers: StreamMap::new(BTreeMap::new()),
            outbound_stream_number: HashMap::new(),
        }
    }

    /// Run the stream handler indefinitely.
    pub async fn run(mut self) -> Result<Never, StreamHandlerError> {
        loop {
            self.handle_next_msg().await?
        }
    }

    /// Listen for a single message coming from the network or from an application.
    /// - Outbound messages are wrapped as StreamMessage and sent to the network directly.
    /// - Inbound messages are stripped of StreamMessage and buffered until they can be sent in the
    ///   correct order to the application.
    ///
    /// Expects to live forever, returning an Error if the client or network close their sender.
    pub async fn handle_next_msg(&mut self) -> Result<(), StreamHandlerError> {
        tokio::select!(
            // New outbound stream.
            outbound_stream = self.outbound_channel_receiver.next() => {
                self.handle_new_stream(outbound_stream).await
            }
            // New message on an existing outbound stream.
            output = self.outbound_stream_receivers.next() => {
                self.handle_outbound_message(output).await;
                Ok(())
            }
            // New inbound message from the network.
            message = self.inbound_receiver.next() => {
                self.handle_inbound_message(message)
            }
        )
    }

    async fn handle_new_stream(
        &mut self,
        outbound_stream: Option<(StreamId, mpsc::Receiver<StreamContent>)>,
    ) -> Result<(), StreamHandlerError> {
        let Some((stream_id, receiver)) = outbound_stream else {
            warn!("Outbound streams channel closed. No new outbound streams can be started.");
            return Err(StreamHandlerError::OutboundChannelClosed);
        };
        if self.outbound_stream_receivers.insert(stream_id.clone(), receiver).is_some() {
            warn!(%stream_id, "Outbound stream ID reused.");
            return Err(StreamHandlerError::StreamIdReused(format!("{stream_id}")));
        }
        CONSENSUS_OUTBOUND_STREAM_STARTED.increment(1);
        info!(%stream_id, "Outbound stream started.");
        Ok(())
    }

    async fn handle_outbound_message(
        &mut self,
        message: Option<(StreamId, Option<StreamContent>)>,
    ) {
        match message {
            Some((key, Some(msg))) => self.broadcast(key, msg).await,
            Some((key, None)) => self.broadcast_fin(key).await,
            None => {
                panic!("StreamHashMap should never be closed")
            }
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
                        panic!("Unexpected error: {e:?}");
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
        CONSENSUS_OUTBOUND_STREAM_FINISHED.increment(1);
        info!(%stream_id, "Outbound stream finished.");
    }

    // Handle a message that was received from the network.
    #[instrument(skip_all, level = "warn")]
    #[allow(clippy::type_complexity)]
    fn handle_inbound_message(
        &mut self,
        message: Option<(
            Result<StreamMessage<StreamContent, StreamId>, ProtobufConversionError>,
            BroadcastedMessageMetadata,
        )>,
    ) -> Result<(), StreamHandlerError> {
        let (message, metadata) = match message {
            None => return Err(StreamHandlerError::InboundChannelClosed),
            Some((Ok(message), metadata)) => (message, metadata),
            Some((Err(e), _)) => {
                // TODO(guy): switch to debug when network is opened to "all".
                warn!("Error converting message: {:?}", e);
                return Ok(());
            }
        };

        let peer_id = metadata.originator_id.clone();
        let stream_id = message.stream_id.clone();
        let key = (peer_id.clone(), stream_id.clone());

        // Try to get the stream data from the cache.
        let data = match self.inbound_stream_data.pop(&key) {
            Some(data) => data,
            None => {
                info!(?peer_id, ?stream_id, "Inbound stream started");
                CONSENSUS_INBOUND_STREAM_STARTED.increment(1);
                StreamData::new(self.config.channel_buffer_capacity)
            }
        };
        if let Some(data) = self.handle_message_inner(message, metadata, data) {
            if let Some((evicted_key, _)) = self.inbound_stream_data.push(key, data) {
                CONSENSUS_INBOUND_STREAM_EVICTED.increment(1);
                warn!(?evicted_key, "Evicted inbound stream due to capacity");
            }
        }
        Ok(())
    }

    /// Returns the StreamData struct if it should be put back into the LRU cache. None if the data
    /// should be dropped.
    fn handle_message_inner(
        &mut self,
        message: StreamMessage<StreamContent, StreamId>,
        metadata: BroadcastedMessageMetadata,
        mut data: StreamData<StreamContent, StreamId>,
    ) -> Option<StreamData<StreamContent, StreamId>> {
        let peer_id = metadata.originator_id;
        let stream_id = message.stream_id.clone();
        let key = (peer_id.clone(), stream_id.clone());
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
                    CONSENSUS_INBOUND_STREAM_FINISHED.increment(1);
                    info!(?peer_id, ?stream_id, "Inbound stream finished.");
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
