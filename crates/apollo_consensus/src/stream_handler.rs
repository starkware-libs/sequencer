//! Overlay streaming logic onto individual messages.

use std::cmp::Ordering;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::num::NonZeroUsize;

use apollo_consensus_config::config::StreamHandlerConfig;
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

use crate::metrics::{
    CONSENSUS_INBOUND_PEER_EVICTED,
    CONSENSUS_INBOUND_STREAM_BUFFER_FULL,
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

/// Errors from the store function, when caching messages.
#[derive(thiserror::Error, PartialEq, Debug)]
enum InternalMessageCacheError {
    /// Too many messages were inserted into the cache for a stream.
    #[error("Too many messages were inserted into the cache for a stream. message_id={0}")]
    TooManyMessages(MessageId),
    /// Duplicate message ID in buffer.
    #[error("Duplicate message ID in buffer. message_id={0}")]
    DuplicateMessageId(MessageId),
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

type PeerStreamDataMap<StreamContent, StreamId> =
    LruCache<PeerId, LruCache<StreamId, StreamData<StreamContent, StreamId>>>;

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
    // An LRU cache mapping for peer_id, each containing an LRU cache on stream_id, mapping each
    // peer_id / stream_id to a struct that contains all the information about the stream. This
    // includes both the message buffer and some metadata (like the latest message ID).
    inbound_stream_data: PeerStreamDataMap<StreamContent, StreamId>,
    // Whenever application wants to start a new stream, it must send out a
    // (stream_id, Receiver) pair. Each receiver gets messages that should
    // be sent out to the network.
    outbound_channel_receiver: mpsc::Receiver<(StreamId, mpsc::Receiver<StreamContent>)>,
    // A map where the above mentioned Receivers are stored.
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
        let cache =
            LruCache::new(NonZeroUsize::new(config.max_peers).expect("max_peers must be non-zero"));

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
        // TODO(Dafna): Consider spawning a separate task for each of the three channels to ensure
        // they don’t block one another.
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
                panic!(
                    "StreamMap should never yield None. When it's empty, it should yield Pending"
                )
            }
        }
    }

    // Returns true if the message was successfully sent.
    // If the message was not sent, it is either due to disconnected channel or full channel.
    fn inbound_send(
        &mut self,
        data: &mut StreamData<StreamContent, StreamId>,
        message: StreamMessage<StreamContent, StreamId>,
    ) -> bool {
        let content = match message.message {
            StreamMessageBody::Content(content) => content,
            // A Fin message is not sent. This is a no-op, can safely return false.
            StreamMessageBody::Fin => return false,
        };

        let sender = &mut data.sender;
        if let Err(e) = sender.try_send(content) {
            warn!(
                "Error sending inbound message: {e:?}; dropping the message. StreamId: {}, \
                 MessageId: {}",
                message.stream_id, message.message_id
            );
            return false;
        }

        // Send the receiver only once the first message has been sent.
        if message.message_id == 0 {
            // If this is the first message, send the receiver to the application.
            // Note: By this point, messages must be unique. Duplicate message IDs should
            // have been discarded earlier.
            let receiver = data.receiver.take().expect(
                "There can't be two messages with message_id=0, as we make sure messages come in \
                 order into here",
            );

            // Send the receiver to the application.
            let send_result = self.inbound_channel_sender.try_send(receiver);
            if let Err(e) = send_result {
                if e.is_disconnected() {
                    panic!("Application dropped inbound_channel_sender's receiver");
                } else {
                    // The channel is full.
                    warn!(
                        "Failed to send receiver to application: {e:?}; dropping the message. \
                         StreamId: {}, MessageId: {}",
                        message.stream_id, message.message_id
                    );
                    return false;
                }
            }
        }
        data.next_message_id += 1;
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
        if let Err(e) = self.outbound_sender.broadcast_message(message).await {
            warn!(%stream_id, "Failed to broadcast outbound stream message: {e:?}. Dropping the message.");
        }
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
        if let Err(e) = self.outbound_sender.broadcast_message(message).await {
            warn!(%stream_id, "Failed to broadcast outbound stream Fin: {e:?}. Dropping the message.");
        }
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

        // Scope the mutable borrow of inbound_stream_data so we can call self.handle_message_inner
        // below.
        let data_option = {
            // Try to get the stream data from the cache. If peer/stread doesn't exist, return None.
            if let Some(per_peer_cache) = self.inbound_stream_data.get_mut(&peer_id) {
                per_peer_cache.pop(&stream_id)
            } else {
                None
            }
        };

        let data = match data_option {
            Some(data) => data,
            None => {
                info!(?peer_id, ?stream_id, "Inbound stream started");
                CONSENSUS_INBOUND_STREAM_STARTED.increment(1);
                StreamData::new(self.config.channel_buffer_capacity)
            }
        };

        // If the stream finishes or fails for any reason, None is returned.
        let Some(data) = self.handle_message_inner(message, metadata, data) else {
            return Ok(());
        };
        // Check if we need to start a new LRU cache for this peer.
        if !self.inbound_stream_data.contains(&peer_id) {
            let evicted_peer_id = self.inbound_stream_data.push(
                peer_id.clone(),
                LruCache::new(
                    NonZeroUsize::new(self.config.max_streams)
                        .expect("max_streams must be non-zero"),
                ),
            );
            if let Some((evicted_peer_id, _)) = evicted_peer_id {
                warn!(?evicted_peer_id, "Evicted peer due to capacity");
                CONSENSUS_INBOUND_PEER_EVICTED.increment(1);
            }
        }
        let per_peer_cache = self
            .inbound_stream_data
            .get_mut(&peer_id)
            .expect("Cache for this peer_id is checked or added above");
        if let Some((evicted_key, _)) = per_peer_cache.push(stream_id, data) {
            CONSENSUS_INBOUND_STREAM_EVICTED.increment(1);
            warn!(?peer_id, ?evicted_key, "Evicted inbound stream due to capacity");
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
                    warn!(?peer_id, ?stream_id, ?message_id, ?data.max_message_id_received,
                        "Received fin message with id that is smaller than a previous message!");
                    return None;
                }
            }
        }

        if message_id > data.fin_message_id.unwrap_or(u64::MAX) {
            // TODO(guyn): replace warnings with more graceful error handling
            warn!(?peer_id, ?stream_id, ?message_id, ?data.fin_message_id,
                "Received message with id that is bigger than the id of the fin message!");
            return None;
        }

        // This means we can just send the message without buffering it.
        match message_id.cmp(&data.next_message_id) {
            Ordering::Equal => {
                let mut message_sent = self.inbound_send(&mut data, message);
                if message_sent {
                    message_sent = self.process_buffer(&mut data);
                }

                // We are done with this stream if:
                // 1. All messages were sent successfully.
                // 2. A send error occurred (receiver dropped or channel full).
                //
                // A full channel is currently treated as an error, as we expect
                // capacity to always be sufficient.
                // TODO(Dafna): Consider implementing buffering as a fallback for the 'full' case.
                if data.message_buffer.is_empty()
                    && data.fin_message_id.is_some()
                    && data.fin_message_id.unwrap() == data.next_message_id
                    || !message_sent
                {
                    data.sender.close_channel();
                    CONSENSUS_INBOUND_STREAM_FINISHED.increment(1);
                    info!(?peer_id, ?stream_id, "Inbound stream finished.");
                    return None;
                }
            }
            Ordering::Greater => {
                if let Err(e) = Self::store(&mut data, self.config.max_message_buffer_size, message)
                {
                    match e {
                        InternalMessageCacheError::TooManyMessages(message_id) => {
                            warn!(
                                ?peer_id,
                                ?stream_id,
                                ?message_id,
                                "Error storing message in buffer, buffer is full!"
                            );
                            CONSENSUS_INBOUND_STREAM_BUFFER_FULL.increment(1);
                            // If buffer is full, the stream will likely never be finished, so we
                            // drop it.
                            return None;
                        }
                        InternalMessageCacheError::DuplicateMessageId(message_id) => {
                            // TODO(guyn): replace warnings with more graceful error handling
                            warn!(
                                ?peer_id,
                                ?stream_id,
                                ?message_id,
                                "Two messages with the same message_id in buffer!"
                            );
                            // Note that this does not evict the stream!
                        }
                    }
                }
            }
            Ordering::Less => {
                // TODO(guyn): replace warnings with more graceful error handling
                warn!(?peer_id, ?stream_id, ?message_id, ?data.next_message_id,
                    "Received message with id that is smaller than the next message expected!");
                return None;
            }
        }
        Some(data)
    }

    // Store an inbound message in the buffer.
    fn store(
        data: &mut StreamData<StreamContent, StreamId>,
        max_message_buffer_size: usize,
        message: StreamMessage<StreamContent, StreamId>,
    ) -> Result<(), InternalMessageCacheError> {
        // Do not store Fin messages (and don't count them in cache capacity).
        if let StreamMessageBody::Fin = message.message {
            return Ok(());
        }
        let message_id = message.message_id;
        let buffer_len = data.message_buffer.len();
        match data.message_buffer.entry(message_id) {
            Vacant(e) => {
                if buffer_len >= max_message_buffer_size {
                    return Err(InternalMessageCacheError::TooManyMessages(message_id));
                }
                e.insert(message);
                Ok(())
            }
            Occupied(_) => Err(InternalMessageCacheError::DuplicateMessageId(message_id)),
        }
    }

    // Tries to drain as many messages as possible from the buffer (in order),
    // Returns true if all attempted messages were sent successfully, and false otherwise.
    // DOES NOT guarantee that the buffer will be empty after calling this function - only that the
    // messages which were tried to send were handled successfully.
    fn process_buffer(&mut self, data: &mut StreamData<StreamContent, StreamId>) -> bool {
        while let Some(message) = data.message_buffer.remove(&data.next_message_id) {
            if !self.inbound_send(data, message) {
                return false;
            }
        }
        true
    }
}
