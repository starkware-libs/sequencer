//! Propeller network behaviour implementation.

use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll, Waker};

use libp2p::core::Endpoint;
use libp2p::identity::{Keypair, PeerId, PublicKey};
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished, FromSwarm};
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionId,
    NetworkBehaviour,
    NotifyHandler,
    THandler,
    THandlerInEvent,
    THandlerOutEvent,
    ToSwarm,
};

use crate::config::Config;
use crate::handler::{Handler, HandlerIn, HandlerOut};
use crate::message::PropellerMessage;
use crate::metrics::PropellerMetrics;
use crate::reed_solomon::{generate_coding_shards, split_data_into_shards};
use crate::tree::PropellerTreeManager;
use crate::types::{
    Event,
    MessageRoot,
    PeerSetError,
    ReconstructionError,
    ShardIndex,
    ShardPublishError,
    ShardSignatureVerificationError,
    ShardValidationError,
};
use crate::{signature, MerkleTree, ValidationMode};

enum MessageReceivingState {
    Receiving { received_shards: Vec<PropellerMessage>, was_my_shard_broadcasted: bool },
    Built { un_padded_message: Vec<u8> },
    Received,
    Denied,
}

struct MessageReceivingMetadata {
    signature: Vec<u8>,
    received_shards: HashSet<ShardIndex>,
    state: MessageReceivingState,
}

/// Determines the authenticity requirements for messages.
///
/// This controls how messages are signed and validated in the Propeller protocol.
#[derive(Clone)]
pub enum MessageAuthenticity {
    /// Message signing is enabled. The author will be the owner of the key.
    Signed(Keypair),
    /// Message signing is disabled.
    ///
    /// The specified [`PeerId`] will be used as the author of all published messages.
    Author(PeerId),
}

/// The Propeller network behaviour.
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,

    /// Events to be returned to the swarm.
    events: VecDeque<ToSwarm<Event, HandlerIn>>,

    /// Waker for the behaviour.
    waker: Option<Waker>,

    /// Tree manager for computing topology per-publisher.
    tree_manager: PropellerTreeManager,

    /// Currently connected peers.
    connected_peers: HashSet<PeerId>,

    /// Message authenticity configuration for signing/verification.
    message_authenticity: MessageAuthenticity,

    /// Map of peer IDs to their public keys for signature verification.
    peer_public_keys: HashMap<PeerId, PublicKey>,

    /// Verified shards organized by (publisher, message_id).
    message_states: lru_time_cache::LruCache<(PeerId, MessageRoot), MessageReceivingMetadata>,

    /// Optional metrics for monitoring and profiling.
    metrics: Option<PropellerMetrics>,
}

impl Behaviour {
    /// Helper to convert usize to u64 safely for metrics
    #[allow(clippy::as_conversions)]
    fn to_u64(value: usize) -> u64 {
        value as u64
    }

    /// Helper to convert usize to f64 safely for metrics
    #[allow(clippy::as_conversions)]
    fn to_f64(value: usize) -> f64 {
        value as f64
    }

    /// Create a new Propeller behaviour.
    pub fn new(message_authenticity: MessageAuthenticity, config: Config) -> Self {
        Self::new_with_metrics(message_authenticity, config, None)
    }

    /// Create a new Propeller behaviour with optional metrics.
    pub fn new_with_metrics(
        message_authenticity: MessageAuthenticity,
        config: Config,
        metrics: Option<PropellerMetrics>,
    ) -> Self {
        let local_peer_id = match &message_authenticity {
            MessageAuthenticity::Signed(keypair) => PeerId::from(keypair.public()),
            MessageAuthenticity::Author(peer_id) => *peer_id,
        };

        let message_states =
            lru_time_cache::LruCache::with_expiry_duration(config.message_cache_ttl());

        Self {
            tree_manager: PropellerTreeManager::new(local_peer_id),
            config,
            waker: None,
            events: VecDeque::new(),
            connected_peers: HashSet::new(),
            message_authenticity,
            peer_public_keys: HashMap::new(),
            message_states,
            metrics,
        }
    }

    /// Add multiple peers with their weights for tree topology calculation.
    ///
    /// This method allows you to add multiple peers at once, each with an associated weight
    /// that determines their position in the dissemination tree. Higher weight peers are
    /// positioned closer to the root, making them more likely to receive messages earlier.
    pub fn set_peers(
        &mut self,
        peers: impl IntoIterator<Item = (PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        self.set_peers_and_optional_keys(
            peers.into_iter().map(|(peer_id, weight)| (peer_id, weight, None)),
        )
    }

    /// Set the list of peers with explicit public keys for signature verification.
    pub fn set_peers_and_keys(
        &mut self,
        peers: impl IntoIterator<Item = (PeerId, u64, PublicKey)>,
    ) -> Result<(), PeerSetError> {
        self.set_peers_and_optional_keys(
            peers
                .into_iter()
                .map(|(peer_id, weight, public_key)| (peer_id, weight, Some(public_key))),
        )
    }

    /// Set the list of peers with optional public keys.
    pub fn set_peers_and_optional_keys(
        &mut self,
        peers: impl IntoIterator<Item = (PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let mut peer_weights = Vec::new();
        let mut peer_public_keys = HashMap::new();
        for (peer_id, weight, public_key) in peers {
            let public_key = self.get_public_key(peer_id, public_key)?;
            peer_weights.push((peer_id, weight));
            peer_public_keys.insert(peer_id, public_key);
        }

        self.peer_public_keys = peer_public_keys;
        self.tree_manager.update_nodes(peer_weights)?;
        Ok(())
    }

    /// Add a peer with its explicit public key for signature verification.
    fn get_public_key(
        &self,
        peer_id: PeerId,
        public_key: Option<PublicKey>,
    ) -> Result<PublicKey, PeerSetError> {
        if let Some(public_key) = public_key {
            if signature::validate_public_key_matches_peer_id(&public_key, &peer_id) {
                Ok(public_key)
            } else {
                Err(PeerSetError::InvalidPublicKey)
            }
        } else if let Some(extracted_key) = signature::try_extract_public_key_from_peer_id(&peer_id)
        {
            Ok(extracted_key)
        } else {
            Err(PeerSetError::InvalidPublicKey)
        }
    }

    /// Get the number of peers this node knows about (including itself).
    pub fn peer_count(&self) -> usize {
        self.tree_manager.get_node_count()
    }

    fn pad_message(message: Vec<u8>, num_data_shards: usize) -> Vec<u8> {
        let original_message_length: u32 =
            message.len().try_into().expect("Message length too long");
        let amount_to_pad = 2 * num_data_shards - ((message.len() + 4) % (2 * num_data_shards));
        [original_message_length.to_le_bytes().to_vec(), message, vec![0; amount_to_pad]].concat()
    }

    fn un_pad_message(message: Vec<u8>) -> Result<Vec<u8>, ReconstructionError> {
        if message.len() < 4 {
            return Err(ReconstructionError::MessagePaddingError);
        }

        let length_bytes: [u8; 4] = message[..4].try_into().expect("This should never fail");
        let original_message_length: u32 = u32::from_le_bytes(length_bytes);
        let original_message_length_usize: usize = original_message_length.try_into().unwrap();

        if 4 + original_message_length_usize > message.len() {
            return Err(ReconstructionError::MessagePaddingError);
        }

        Ok(message[4..(4 + original_message_length_usize)].to_vec())
    }

    pub fn prepare_messages(
        &self,
        message: Vec<u8>,
    ) -> Result<Vec<PropellerMessage>, ShardPublishError> {
        let num_data_shards = self.tree_manager.calculate_data_shards();
        let message =
            if self.config.pad() { Self::pad_message(message, num_data_shards) } else { message };
        let num_coding_shards = self.tree_manager.calculate_coding_shards();
        let data_shards = split_data_into_shards(message, num_data_shards)
            .ok_or(ShardPublishError::InvalidDataSize)?;
        let coding_shards = generate_coding_shards(&data_shards, num_coding_shards)
            .map_err(ShardPublishError::ErasureEncodingFailed)?;
        let all_shards = [data_shards, coding_shards].concat();
        let merkle_tree = MerkleTree::new(&all_shards);
        let message_root = MessageRoot(merkle_tree.root());
        let signature = match &self.message_authenticity {
            MessageAuthenticity::Signed(keypair) => {
                signature::sign_message_id(&message_root, keypair)?
            }
            MessageAuthenticity::Author(_) => Vec::new(),
        };
        let publisher = self.tree_manager.get_local_peer_id();

        let mut messages = Vec::with_capacity(all_shards.len());
        for (index, shard) in all_shards.into_iter().enumerate() {
            let proof = merkle_tree.prove(index).unwrap();
            let message = PropellerMessage::new(
                message_root,
                publisher,
                signature.clone(),
                ShardIndex(index.try_into().unwrap()),
                shard,
                proof,
            );
            message.validate_shard_proof().unwrap();
            messages.push(message);
        }

        Ok(messages)
    }

    pub fn broadcast_prepared_messages(
        &mut self,
        messages: Vec<PropellerMessage>,
    ) -> Result<(), ShardPublishError> {
        let publisher = self.tree_manager.get_local_peer_id();

        if let Some(metrics) = &self.metrics {
            metrics.shards_published.increment(Self::to_u64(messages.len()));
        }

        for message in messages {
            let shard_index = message.index();
            let peer = self
                .tree_manager
                .get_peer_for_shard_id(&publisher, shard_index)
                .map_err(ShardPublishError::TreeGenerationError)?;
            self.send_message_to_peer(message, peer);
        }

        Ok(())
    }

    pub fn broadcast(&mut self, message: Vec<u8>) -> Result<(), ShardPublishError> {
        let messages = self.prepare_messages(message)?;
        self.broadcast_prepared_messages(messages)
    }

    /// Get a reference to the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Add multiple peers to the connected peers set for testing purposes.
    pub fn add_connected_peers_for_test(&mut self, peer_ids: Vec<PeerId>) {
        for peer_id in peer_ids {
            self.connected_peers.insert(peer_id);
        }
    }

    fn verify_message_signature_cached(
        &mut self,
        message: &PropellerMessage,
    ) -> Result<(), ShardSignatureVerificationError> {
        if let Some(message_metadata) =
            self.message_states.get(&(message.publisher(), message.root()))
        {
            if message_metadata.signature == message.signature() {
                return Ok(());
            } else {
                return Err(ShardSignatureVerificationError::VerificationFailed);
            }
        }
        self.verify_message_signature(message)
    }

    /// Verify the signature of a shard.
    fn verify_message_signature(
        &self,
        message: &PropellerMessage,
    ) -> Result<(), ShardSignatureVerificationError> {
        if self.config.validation_mode() == &ValidationMode::None {
            return Ok(());
        }

        let publisher_id = message.publisher();
        let Some(signer_public_key) = self.peer_public_keys.get(&publisher_id) else {
            return Err(ShardSignatureVerificationError::NoPublicKeyAvailable(publisher_id));
        };

        signature::verify_message_id_signature(
            &message.root(),
            message.signature(),
            signer_public_key,
        )
    }

    fn validate_not_duplicate(
        &mut self,
        message: &PropellerMessage,
    ) -> Result<(), ShardValidationError> {
        if let Some(message_metadata) =
            self.message_states.get(&(message.publisher(), message.root()))
        {
            if message_metadata.received_shards.contains(&message.index()) {
                return Err(ShardValidationError::DuplicateShard);
            }
        }
        Ok(())
    }

    pub fn validate_shard(
        &mut self,
        sender: PeerId,
        message: &PropellerMessage,
    ) -> Result<(), ShardValidationError> {
        self.validate_not_duplicate(message)?;
        self.tree_manager.validate_origin(sender, message)?;
        message.validate_shard_proof()?;
        self.verify_message_signature_cached(message)
            .map_err(ShardValidationError::SignatureVerificationFailed)?;
        Ok(())
    }

    /// Update the message state machine with a newly received shard.
    /// Returns the number of shards received for this message.
    fn update_state(
        &mut self,
        message: PropellerMessage,
    ) -> Result<Option<PropellerMessage>, ReconstructionError> {
        let shard_index = message.index();
        let publisher = message.publisher();
        let message_root = message.root();
        let key = (publisher, message_root);
        let signature = message.signature().to_vec();
        let am_i_shard_publisher =
            self.tree_manager.get_my_shard_index(&publisher).unwrap() == shard_index;

        let metadata = self.message_states.entry(key).or_insert_with(|| MessageReceivingMetadata {
            signature,
            received_shards: HashSet::new(),
            state: MessageReceivingState::Receiving {
                received_shards: Vec::new(),
                was_my_shard_broadcasted: false,
            },
        });

        // Track this shard index
        metadata.received_shards.insert(shard_index);

        // Add the shard to the state
        match &mut metadata.state {
            MessageReceivingState::Receiving { received_shards, was_my_shard_broadcasted } => {
                received_shards.push(message.clone());
                let shard_count = received_shards.len();
                if self.tree_manager.should_build(shard_count) {
                    return self.transition_from_receiving_state(publisher, message_root);
                }

                if am_i_shard_publisher {
                    assert!(
                        !*was_my_shard_broadcasted,
                        "received my shard twice, should have been caught by validation"
                    );
                    *was_my_shard_broadcasted = true;
                    Ok(Some(message))
                } else {
                    Ok(None)
                }
            }
            MessageReceivingState::Built { un_padded_message } => {
                if metadata.received_shards.len() < self.tree_manager.calculate_coding_shards() {
                    return Ok(None);
                }
                let message = std::mem::take(un_padded_message);
                metadata.state = MessageReceivingState::Received;
                self.emit_event(Event::MessageReceived { publisher, message_root, message });
                Ok(None)
            }
            MessageReceivingState::Received | MessageReceivingState::Denied => Ok(None),
        }
    }

    /// Try to build the message from F shards and validate the merkle root.
    fn transition_from_receiving_state(
        &mut self,
        publisher: PeerId,
        message_root: MessageRoot,
    ) -> Result<Option<PropellerMessage>, ReconstructionError> {
        let key = (publisher, message_root);
        let metadata = self.message_states.get_mut(&key).expect("Message metadata must exist");

        // in case of any failure we will assume the message is denied
        let previous_state = std::mem::replace(&mut metadata.state, MessageReceivingState::Denied);

        let MessageReceivingState::Receiving { received_shards, was_my_shard_broadcasted } =
            previous_state
        else {
            unreachable!("Expected Receiving state");
        };

        // Collect shards for reconstruction
        let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_shards
            .iter()
            .map(|msg| (msg.index().0.try_into().unwrap(), msg.shard().to_vec()))
            .collect();

        let data_count = self.tree_manager.calculate_data_shards();
        let coding_count = self.tree_manager.calculate_coding_shards();

        // Reconstruct the data shards using Reed-Solomon
        let reconstructed_data_shards = crate::reed_solomon::reconstruct_message_from_shards(
            &shards_for_reconstruction,
            data_count,
            coding_count,
        )
        .map_err(ReconstructionError::ErasureReconstructionFailed)?;

        // Recreate all shards (data + coding) to validate the merkle root
        let recreated_coding_shards =
            crate::reed_solomon::generate_coding_shards(&reconstructed_data_shards, coding_count)
                .map_err(ReconstructionError::ErasureReconstructionFailed)?;

        let all_shards = [reconstructed_data_shards.clone(), recreated_coding_shards].concat();

        let are_all_shards_the_same_length =
            all_shards.iter().all(|shard| shard.len() == all_shards[0].len());
        if !are_all_shards_the_same_length {
            return Err(ReconstructionError::UnequalShardLengths);
        }

        // Build merkle tree and validate root
        let merkle_tree = MerkleTree::new(&all_shards);
        let computed_root = MessageRoot(merkle_tree.root());

        if computed_root != message_root {
            return Err(ReconstructionError::MismatchedMessageRoot);
        }

        // Validation passed! Transition to Built state
        let message = crate::reed_solomon::combine_data_shards(reconstructed_data_shards);
        let un_padded_message =
            if self.config.pad() { Self::un_pad_message(message)? } else { message };
        let signature = metadata.signature.clone();

        let shard_count = metadata.received_shards.len();
        if self.tree_manager.should_receive(shard_count) {
            metadata.state = MessageReceivingState::Received;
            self.emit_event(Event::MessageReceived {
                publisher,
                message_root,
                message: un_padded_message,
            });
        } else {
            metadata.state = MessageReceivingState::Built { un_padded_message };
        }

        if !was_my_shard_broadcasted {
            let my_shard_index = self.tree_manager.get_my_shard_index(&publisher).unwrap();
            let my_shard_index_usize: usize = my_shard_index.0.try_into().unwrap();
            let my_shard = all_shards[my_shard_index_usize].clone();
            let proof = merkle_tree.prove(my_shard_index_usize).unwrap();
            let my_message = PropellerMessage::new(
                message_root,
                publisher,
                signature,
                my_shard_index,
                my_shard,
                proof,
            );
            return Ok(Some(my_message));
        }

        Ok(None)
    }

    /// Handle a received shard from a peer with full verification.
    fn handle_received_shard(&mut self, sender: PeerId, message: PropellerMessage) {
        // Track received shard
        if let Some(metrics) = &self.metrics {
            metrics.shards_received.increment(1);
            metrics.shard_bytes_received.increment(Self::to_u64(message.shard().len()));
        }

        if let Err(error) = self.validate_shard(sender, &message) {
            if let Some(metrics) = &self.metrics {
                metrics.increment_validation_failure(error.clone().into());
            }
            self.emit_event(Event::ShardValidationFailed {
                sender,
                claimed_root: message.root(),
                claimed_publisher: message.publisher(),
                error,
            });
            return;
        }

        let publisher = message.publisher();
        let message_root = message.root();

        // Track validated shard
        if let Some(metrics) = &self.metrics {
            metrics.shards_validated.increment(1);
        }

        // Emit event for the verified shard
        if self.config.emit_shard_received_events() {
            self.emit_event(Event::ShardReceived {
                publisher,
                shard_index: message.index(),
                sender,
                message_root,
                shard: message.shard().to_vec(),
            });
        }

        // Update state machine
        let result = self.update_state(message);
        let Ok(my_shard) = result else {
            let error = result.unwrap_err();
            self.emit_event(Event::MessageReconstructionFailed { publisher, message_root, error });
            return;
        };

        let local_peer_id = self.tree_manager.get_local_peer_id();
        if let Some(my_shard) = my_shard {
            debug_assert_eq!(my_shard.publisher(), publisher);
            debug_assert_eq!(my_shard.root(), message_root);
            debug_assert_eq!(
                my_shard.index(),
                self.tree_manager.get_my_shard_index(&publisher).unwrap()
            );
            let peers_in_broadcast = self
                .tree_manager
                .get_nodes()
                .iter()
                .map(|(peer, _)| *peer)
                .filter(|peer| *peer != publisher && *peer != local_peer_id)
                .collect::<Vec<_>>();
            for peer in peers_in_broadcast {
                self.send_message_to_peer(my_shard.clone(), peer);
            }
        }
    }

    fn send_message_to_peer(&mut self, message: PropellerMessage, peer: PeerId) {
        if let Some(metrics) = &self.metrics {
            metrics.shards_sent.increment(1);
            metrics.shard_bytes_sent.increment(Self::to_u64(message.shard().len()));
        }
        self.emit_handler_event(peer, HandlerIn::SendMessage(message));
    }

    fn emit_event(&mut self, event: Event) {
        self.events.push_back(ToSwarm::GenerateEvent(event));
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains(&peer_id) {
            if let Some(metrics) = &self.metrics {
                metrics.increment_send_failure(
                    crate::metrics::ShardSendFailureReason::NotConnectedToPeer,
                );
            }
            self.emit_event(Event::ShardSendFailed {
                sent_from: None,
                sent_to: Some(peer_id),
                error: ShardPublishError::NotConnectedToPeer(peer_id),
            });
            return;
        }

        self.events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::Any,
            event,
        });
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = Handler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &libp2p::core::Multiaddr,
        _remote_addr: &libp2p::core::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            self.config.stream_protocol().clone(),
            self.config.max_shard_size(),
            self.config.substream_timeout(),
        ))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &libp2p::core::Multiaddr,
        _role_override: Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Handler::new(
            self.config.stream_protocol().clone(),
            self.config.max_shard_size(),
            self.config.substream_timeout(),
        ))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id: _,
                endpoint: _,
                failed_addresses: _,
                other_established: _,
            }) => {
                self.connected_peers.insert(peer_id);

                // Update connected peers metric
                if let Some(metrics) = &self.metrics {
                    metrics.num_connected_peers.set(Self::to_f64(self.connected_peers.len()));
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id: _,
                endpoint: _,
                remaining_established,
                cause: _,
            }) => {
                if remaining_established == 0 {
                    self.connected_peers.remove(&peer_id);

                    // Update connected peers metric
                    if let Some(metrics) = &self.metrics {
                        metrics.num_connected_peers.set(Self::to_f64(self.connected_peers.len()));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerOut::Message(message) => {
                self.handle_received_shard(peer_id, message);
            }
            HandlerOut::SendError(error) => {
                // Track send failure
                if let Some(metrics) = &self.metrics {
                    metrics.increment_send_failure(
                        crate::metrics::ShardSendFailureReason::HandlerError,
                    );
                }

                self.emit_event(Event::ShardSendFailed {
                    sent_from: None,
                    sent_to: Some(peer_id),
                    error: ShardPublishError::HandlerError(error),
                });
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.waker = Some(cx.waker().clone());

        // Return any pending events
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}
