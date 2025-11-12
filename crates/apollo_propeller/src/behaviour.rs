//! Propeller network behaviour implementation.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

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
use lru_time_cache::LruCache;
use rand::seq::SliceRandom;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::handler::{Handler, HandlerIn, HandlerOut};
use crate::metrics::PropellerMetrics;
use crate::reed_solomon::{generate_coding_shards, split_data_into_shards};
use crate::tree::PropellerTreeManager;
use crate::types::{
    Channel,
    Event,
    MessageRoot,
    PeerSetError,
    ReconstructionError,
    ShardIndex,
    ShardPublishError,
    ShardValidationError,
};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;
use crate::{signature, MerkleProof, MerkleTree};

/// State of message receiving in the parallel processor.
enum MessageReceivingState {
    Receiving { received_shards: Vec<PropellerUnit> },
    Built { un_padded_message: Vec<u8> },
}

/// Handle to a message processor task.
#[derive(Clone)]
struct MessageProcessorHandle {
    tx: mpsc::UnboundedSender<(PeerId, PropellerUnit)>,
}

/// Helper to wake the swarm when results are available.
#[derive(Clone)]
struct WakerNotifier {
    waker: Arc<Mutex<Option<Waker>>>,
}

impl WakerNotifier {
    fn new() -> Self {
        Self { waker: Arc::new(Mutex::new(None)) }
    }

    fn set_waker(&self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }

    fn wake(&self) {
        if let Some(waker) = self.waker.lock().unwrap().as_ref() {
            waker.wake_by_ref();
        }
    }
}

/// Results from parallel message processing tasks.
enum ProcessorResult {
    /// Shard validated successfully
    ShardValidated {
        channel: Channel,
        sender: PeerId,
        publisher: PeerId,
        message_root: MessageRoot,
        shard_index: ShardIndex,
    },

    /// My shard is ready - need to broadcast to peers
    BroadcastMyShard {
        channel: Channel,
        publisher: PeerId,
        message_root: MessageRoot,
        my_shard: PropellerUnit,
        broadcast_to: Vec<PeerId>,
    },

    /// Message fully reconstructed
    MessageReconstructed {
        channel: Channel,
        publisher: PeerId,
        message_root: MessageRoot,
        message: Vec<u8>,
    },

    /// Validation failed
    ValidationFailed {
        channel: Channel,
        sender: PeerId,
        publisher: PeerId,
        message_root: MessageRoot,
        error: ShardValidationError,
    },

    /// Reconstruction failed
    ReconstructionFailed {
        channel: Channel,
        publisher: PeerId,
        message_root: MessageRoot,
        error: ReconstructionError,
    },
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

/// Data associated with a single channel.
struct ChannelData {
    /// Tree manager for computing topology for this channel.
    tree_manager: Arc<PropellerTreeManager>,
    /// Map of peer IDs to their public keys for signature verification.
    peer_public_keys: HashMap<PeerId, PublicKey>,
    /// Active message processors for this channel - one task per (publisher, message_root).
    active_processors: HashMap<(PeerId, MessageRoot), MessageProcessorHandle>,
    /// Messages that were either received or rejected on this channel.
    finalized_messages: LruCache<(PeerId, MessageRoot), ()>,
}

impl ChannelData {
    fn new(
        tree_manager: Arc<PropellerTreeManager>,
        peer_public_keys: HashMap<PeerId, PublicKey>,
        finalized_message_ttl: Duration,
    ) -> Self {
        Self {
            tree_manager,
            peer_public_keys,
            active_processors: HashMap::new(),
            finalized_messages: LruCache::with_expiry_duration(finalized_message_ttl),
        }
    }
}

/// Manages all channels and their associated data.
pub struct ChannelManager {
    /// Local peer ID.
    local_peer_id: PeerId,
    /// TTL for finalized messages cache.
    finalized_message_ttl: Duration,
    /// All registered channels.
    channels: HashMap<Channel, ChannelData>,
}

impl ChannelManager {
    /// Create a new channel manager.
    fn new(local_peer_id: PeerId, finalized_message_ttl: Duration) -> Self {
        Self { local_peer_id, finalized_message_ttl, channels: HashMap::new() }
    }

    /// Register a channel with peers and optional public keys.
    fn register_channel(
        &mut self,
        channel: Channel,
        peer_weights: Vec<(PeerId, u64)>,
        peer_public_keys: HashMap<PeerId, PublicKey>,
    ) -> Result<(), PeerSetError> {
        let mut new_tree_manager = PropellerTreeManager::new(self.local_peer_id);
        new_tree_manager.update_nodes(peer_weights)?;

        let channel_data = ChannelData::new(
            Arc::new(new_tree_manager),
            peer_public_keys,
            self.finalized_message_ttl,
        );

        self.channels.insert(channel, channel_data);
        Ok(())
    }

    /// Get tree manager for a channel.
    fn get_tree_manager(&self, channel: &Channel) -> Option<&Arc<PropellerTreeManager>> {
        self.channels.get(channel).map(|data| &data.tree_manager)
    }

    /// Get peer public keys for a channel.
    fn get_peer_public_keys(&self, channel: &Channel) -> Option<&HashMap<PeerId, PublicKey>> {
        self.channels.get(channel).map(|data| &data.peer_public_keys)
    }

    /// Check if a channel is registered.
    fn is_channel_registered(&self, channel: &Channel) -> bool {
        self.channels.contains_key(channel)
    }

    /// Get all registered channels.
    fn registered_channels(&self) -> Vec<Channel> {
        self.channels.keys().copied().collect()
    }

    /// Get peer count for a channel.
    fn peer_count(&self, channel: &Channel) -> Option<usize> {
        self.get_tree_manager(channel).map(|tm| tm.get_node_count())
    }

    /// Check if a message is finalized on a channel.
    fn is_message_finalized(
        &mut self,
        channel: &Channel,
        publisher: &PeerId,
        root: &MessageRoot,
    ) -> bool {
        self.channels
            .get_mut(channel)
            .and_then(|data| data.finalized_messages.get(&(*publisher, *root)))
            .is_some()
    }

    /// Mark a message as finalized on a channel.
    fn mark_message_finalized(&mut self, channel: Channel, publisher: PeerId, root: MessageRoot) {
        if let Some(data) = self.channels.get_mut(&channel) {
            data.finalized_messages.insert((publisher, root), ());
        }
    }

    /// Check if a processor exists for a message.
    fn has_processor(&self, channel: &Channel, publisher: &PeerId, root: &MessageRoot) -> bool {
        self.channels
            .get(channel)
            .map(|data| data.active_processors.contains_key(&(*publisher, *root)))
            .unwrap_or(false)
    }

    /// Insert a processor handle for a message.
    fn insert_processor(
        &mut self,
        channel: Channel,
        publisher: PeerId,
        root: MessageRoot,
        tx: mpsc::UnboundedSender<(PeerId, PropellerUnit)>,
    ) {
        if let Some(data) = self.channels.get_mut(&channel) {
            data.active_processors.insert((publisher, root), MessageProcessorHandle { tx });
        }
    }

    /// Get a processor handle for a message.
    fn get_processor(
        &self,
        channel: &Channel,
        publisher: &PeerId,
        root: &MessageRoot,
    ) -> Option<&MessageProcessorHandle> {
        self.channels.get(channel).and_then(|data| data.active_processors.get(&(*publisher, *root)))
    }

    /// Remove a processor for a message.
    fn remove_processor(&mut self, channel: &Channel, publisher: &PeerId, root: &MessageRoot) {
        if let Some(data) = self.channels.get_mut(channel) {
            data.active_processors.remove(&(*publisher, *root));
        }
    }

    /// Get the number of active processors across all channels.
    fn total_active_processors(&self) -> usize {
        self.channels.values().map(|data| data.active_processors.len()).sum()
    }

    /// Get the total number of finalized messages across all channels.
    fn total_finalized_messages(&self) -> usize {
        self.channels.values().map(|data| data.finalized_messages.len()).sum()
    }

    /// Get the number of registered channels.
    fn num_channels(&self) -> usize {
        self.channels.len()
    }
}

/// The Propeller network behaviour.
pub struct Behaviour {
    /// Configuration for this behaviour.
    config: Config,

    /// Events to be returned to the swarm.
    events: VecDeque<ToSwarm<Event, HandlerIn>>,

    /// Channel manager for all registered channels.
    channel_manager: ChannelManager,

    /// Currently connected peers.
    connected_peers: HashMap<PeerId, Vec<ConnectionId>>,

    /// Message authenticity configuration for signing/verification.
    message_authenticity: MessageAuthenticity,

    /// Local peer ID derived from message authenticity.
    local_peer_id: PeerId,

    /// Results from parallel processing tasks.
    processor_results_rx: mpsc::UnboundedReceiver<ProcessorResult>,
    processor_results_tx: mpsc::UnboundedSender<ProcessorResult>,

    /// Channels from the broadcaster tasks.
    broadcaster_results_rx: mpsc::UnboundedReceiver<Result<Vec<PropellerUnit>, ShardPublishError>>,
    broadcaster_results_tx: mpsc::UnboundedSender<Result<Vec<PropellerUnit>, ShardPublishError>>,

    /// Waker notifier to wake the swarm when processor results arrive.
    waker_notifier: WakerNotifier,

    /// Optional metrics for monitoring and profiling.
    metrics: Option<Arc<PropellerMetrics>>,
}

impl Behaviour {
    /// Helper to convert usize to u64 safely for metrics
    #[allow(clippy::as_conversions)]
    fn to_u64(value: usize) -> u64 {
        value as u64
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
        let metrics = metrics.map(Arc::new);
        let local_peer_id = match &message_authenticity {
            MessageAuthenticity::Signed(keypair) => PeerId::from(keypair.public()),
            MessageAuthenticity::Author(peer_id) => *peer_id,
        };

        let (processor_results_tx, processor_results_rx) = mpsc::unbounded_channel();
        let (broadcaster_results_tx, broadcaster_results_rx) = mpsc::unbounded_channel();

        Self {
            channel_manager: ChannelManager::new(local_peer_id, config.finalized_message_ttl()),
            config,
            events: VecDeque::new(),
            connected_peers: HashMap::new(),
            message_authenticity,
            local_peer_id,
            processor_results_rx,
            processor_results_tx,
            broadcaster_results_rx,
            broadcaster_results_tx,
            waker_notifier: WakerNotifier::new(),
            metrics,
        }
    }

    /// Register a channel with multiple peers and their weights for tree topology calculation.
    ///
    /// This method allows you to register a channel with multiple peers at once, each with an
    /// associated weight that determines their position in the dissemination tree. Higher weight
    /// peers are positioned closer to the root, making them more likely to receive messages
    /// earlier.
    pub fn register_channel_peers(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64)>,
    ) -> Result<(), PeerSetError> {
        self.register_channel_peers_and_optional_keys(
            channel,
            peers.into_iter().map(|(peer_id, weight)| (peer_id, weight, None)),
        )
    }

    /// Register a channel with peers and explicit public keys for signature verification.
    pub fn register_channel_peers_and_keys(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64, PublicKey)>,
    ) -> Result<(), PeerSetError> {
        self.register_channel_peers_and_optional_keys(
            channel,
            peers
                .into_iter()
                .map(|(peer_id, weight, public_key)| (peer_id, weight, Some(public_key))),
        )
    }

    /// Register a channel with peers and optional public keys.
    pub fn register_channel_peers_and_optional_keys(
        &mut self,
        channel: Channel,
        peers: impl IntoIterator<Item = (PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let mut peer_weights = Vec::new();
        let mut peer_public_keys = HashMap::new();

        for (peer_id, weight, public_key) in peers {
            let public_key = self.get_public_key(peer_id, public_key)?;
            peer_weights.push((peer_id, weight));
            peer_public_keys.insert(peer_id, public_key);
        }

        // Register the channel with the channel manager
        // Old processors keep their Arc to the old tree (safe - they're mid-processing)
        self.channel_manager.register_channel(channel, peer_weights, peer_public_keys)?;

        // Track tree generation and update channel count
        if let Some(metrics) = &self.metrics {
            metrics.trees_generated.increment(1);
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::RegisteredChannels,
                self.channel_manager.num_channels(),
            );
        }

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

    /// Get the number of peers this node knows about on a specific channel (including itself).
    /// Returns None if the channel is not registered.
    pub fn peer_count(&self, channel: Channel) -> Option<usize> {
        self.channel_manager.peer_count(&channel)
    }

    /// Get all registered channels.
    pub fn registered_channels(&self) -> Vec<Channel> {
        self.channel_manager.registered_channels()
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

    pub fn prepare_units(
        channel: Channel,
        publisher: PeerId,
        keypair: Option<Keypair>,
        message: Vec<u8>,
        pad: bool,
        num_data_shards: usize,
        num_coding_shards: usize,
    ) -> Result<Vec<PropellerUnit>, ShardPublishError> {
        let message = if pad { Self::pad_message(message, num_data_shards) } else { message };
        let data_shards = split_data_into_shards(message, num_data_shards)
            .ok_or(ShardPublishError::InvalidDataSize)?;
        let coding_shards = generate_coding_shards(&data_shards, num_coding_shards)
            .map_err(ShardPublishError::ErasureEncodingFailed)?;
        let all_shards = [data_shards, coding_shards].concat();
        let merkle_tree = MerkleTree::new(&all_shards);
        let message_root = MessageRoot(merkle_tree.root());
        let signature = match keypair {
            Some(keypair) => signature::sign_message_id(&message_root, &keypair)?,
            None => Vec::new(),
        };

        let mut messages = Vec::with_capacity(all_shards.len());
        for (index, shard) in all_shards.into_iter().enumerate() {
            let proof = merkle_tree.prove(index).unwrap();
            let message = PropellerUnit::new(
                channel,
                publisher,
                message_root,
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

    pub fn broadcast_prepared_units(
        &mut self,
        units: Vec<PropellerUnit>,
    ) -> Result<(), ShardPublishError> {
        if units.is_empty() {
            return Ok(());
        }

        let channel = units[0].channel();
        let publisher = self.local_peer_id;

        let tree_manager = self
            .channel_manager
            .get_tree_manager(&channel)
            .ok_or(ShardPublishError::ChannelNotRegistered(channel))?;

        if let Some(metrics) = &self.metrics {
            metrics.shards_published.increment(Self::to_u64(units.len()));
        }

        let peers_in_order = tree_manager.make_first_broadcast_list();
        debug_assert_eq!(peers_in_order.len(), units.len());

        for (message, (peer, shard_index)) in units.into_iter().zip(peers_in_order) {
            debug_assert_eq!(message.publisher(), publisher);
            debug_assert_eq!(message.channel(), channel);
            debug_assert_eq!(message.index(), shard_index);
            self.send_unit_to_peer(message, peer);
        }

        Ok(())
    }

    pub fn broadcast(
        &mut self,
        channel: Channel,
        message: Vec<u8>,
    ) -> Result<tokio::task::JoinHandle<Option<MessageRoot>>, ShardPublishError> {
        let tree_manager = self
            .channel_manager
            .get_tree_manager(&channel)
            .ok_or(ShardPublishError::ChannelNotRegistered(channel))?;

        let publisher = self.local_peer_id;
        let keypair = match &self.message_authenticity {
            MessageAuthenticity::Signed(keypair) => Some(keypair.clone()),
            MessageAuthenticity::Author(_) => None,
        };
        let pad = self.config.pad();
        let num_data_shards = tree_manager.calculate_data_shards();
        let num_coding_shards = tree_manager.calculate_coding_shards();
        let tx = self.broadcaster_results_tx.clone();

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        rayon::spawn(move || {
            let r = Self::prepare_units(
                channel,
                publisher,
                keypair,
                message,
                pad,
                num_data_shards,
                num_coding_shards,
            );
            let task_result = if let Ok(units) = &r { Some(units[0].root()) } else { None };
            tx.send(r).unwrap();
            let _ = result_tx.send(task_result);
        });
        Ok(tokio::spawn(async move { result_rx.await.unwrap_or(None) }))
    }

    fn re_build_message(
        received_shards: Vec<PropellerUnit>,
        message_root: MessageRoot,
        my_shard_index: usize,
        data_count: usize,
        coding_count: usize,
    ) -> Result<(Vec<u8>, Vec<u8>, MerkleProof), ReconstructionError> {
        // Collect shards for reconstruction
        let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_shards
            .into_iter()
            .map(|mut msg| (msg.index().0.try_into().unwrap(), std::mem::take(msg.shard_mut())))
            .collect();

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

        let mut all_shards = [reconstructed_data_shards.clone(), recreated_coding_shards].concat();

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
        Ok((
            message,
            std::mem::take(&mut all_shards[my_shard_index]),
            merkle_tree.prove(my_shard_index).unwrap(),
        ))
    }

    fn send_unit_to_peer(&mut self, unit: PropellerUnit, peer: PeerId) {
        if let Some(metrics) = &self.metrics {
            metrics.shards_sent.increment(1);
            metrics.shard_bytes_sent.increment(Self::to_u64(unit.shard().len()));
        }
        let message = self.config.malice_modify(peer, unit);
        if let Some(message) = message {
            self.emit_handler_event(peer, HandlerIn::SendUnit(message));
        }
    }

    fn send_unit_to_peers(&mut self, unit: PropellerUnit, mut peers: Vec<PeerId>) {
        // Shuffle the peers to avoid bias in the order of sending, this will stop the incentive for
        // low peer IDs
        peers.shuffle(&mut rand::thread_rng());

        // Send the unit to the peers in the shuffled order
        for peer in peers {
            self.send_unit_to_peer(unit.clone(), peer);
        }
    }

    fn update_collection_metrics(&self) {
        if let Some(metrics) = &self.metrics {
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::EventsQueue,
                self.events.len(),
            );
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::ActiveProcessors,
                self.channel_manager.total_active_processors(),
            );
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::FinalizedMessages,
                self.channel_manager.total_finalized_messages(),
            );
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::RegisteredChannels,
                self.channel_manager.num_channels(),
            );
            metrics.update_collection_length(
                crate::metrics::CollectionLabel::ConnectedPeers,
                self.connected_peers.len(),
            );
        }
    }

    /// Spawn a dedicated task for processing a specific message.
    /// Each (channel, publisher, message_root) gets its own task for lock-free parallelism.
    fn spawn_message_processor(
        &self,
        channel: Channel,
        publisher: PeerId,
        message_root: MessageRoot,
    ) -> mpsc::UnboundedSender<(PeerId, PropellerUnit)> {
        let start_time = std::time::Instant::now();

        let (tx, mut rx) = mpsc::unbounded_channel::<(PeerId, PropellerUnit)>();
        let result_tx = self.processor_results_tx.clone();
        let waker_notifier = self.waker_notifier.clone();

        // Clone data needed for the task
        let tree_manager = Arc::clone(
            self.channel_manager
                .get_tree_manager(&channel)
                .expect("Channel must be registered before processing messages"),
        );
        let config = self.config.clone();
        let publisher_public_key = self
            .channel_manager
            .get_peer_public_keys(&channel)
            .and_then(|keys| keys.get(&publisher))
            .cloned();

        // Get static data for this message processing
        let local_peer_id = tree_manager.get_local_peer_id();
        let my_shard_index = tree_manager.get_my_shard_index(&publisher).unwrap();
        let data_shards = tree_manager.calculate_data_shards();
        let coding_shards = tree_manager.calculate_coding_shards();

        // Create channel for passing validated shards between tasks
        let (validated_tx, mut validated_rx) = mpsc::unbounded_channel::<(PeerId, PropellerUnit)>();

        // Clone data needed for both tasks
        let result_tx_validation = result_tx.clone();
        let waker_notifier_validation = waker_notifier.clone();
        let tree_manager_validation = Arc::clone(&tree_manager);
        let config_validation = config.clone();
        let metrics_validation = self.metrics.clone();

        let result_tx_state = result_tx;
        let waker_notifier_state = waker_notifier;
        let tree_manager_state = tree_manager;
        let config_state = config;
        let metrics_state = self.metrics.clone();

        // Spawn Task 1: Validation
        tokio::spawn(async move {
            tracing::trace!(
                "Validation task started for publisher={}, root={}",
                publisher,
                message_root
            );

            // Wrap validator in Arc<Mutex<>> for thread-safe access from spawn_blocking
            let unit_validator = Arc::new(Mutex::new(UnitValidator::new(
                channel,
                publisher,
                publisher_public_key,
                message_root,
                *config_validation.validation_mode(),
                Arc::clone(&tree_manager_validation),
            )));

            while let Some((sender, message)) = rx.recv().await {
                let message_index = message.index();

                tracing::trace!(
                    "Validator: received shard from sender={}, index={}, publisher={}, root={}",
                    sender,
                    message_index,
                    publisher,
                    message_root
                );

                // Offload CPU-intensive validation to Rayon thread pool
                // Return both the result and the message so we can use it later
                let validator = Arc::clone(&unit_validator);
                let validation_start = std::time::Instant::now();
                let (tx, rx) = tokio::sync::oneshot::channel();
                rayon::spawn(move || {
                    let mut validator = validator.lock().unwrap();
                    let result = validator.validate_shard(sender, &message);
                    let _ = tx.send((result, message));
                });
                let (validation_result, message) = rx.await.unwrap();
                let validation_duration = validation_start.elapsed();

                // Record validation timing
                if let Some(ref metrics) = metrics_validation {
                    metrics.record_validation_duration(validation_duration);
                }

                if let Err(error) = validation_result {
                    tracing::trace!(
                        "Validator: validation failed for index={}, error={:?}",
                        message_index,
                        error
                    );
                    let _ = result_tx_validation.send(ProcessorResult::ValidationFailed {
                        channel,
                        sender,
                        publisher,
                        message_root,
                        error,
                    });
                    waker_notifier_validation.wake();
                    // continue to the next unit
                    continue;
                }

                tracing::trace!("Validator: validation passed for index={}", message_index);

                // Emit validation success
                let _ = result_tx_validation.send(ProcessorResult::ShardValidated {
                    channel,
                    sender,
                    publisher,
                    message_root,
                    shard_index: message_index,
                });
                waker_notifier_validation.wake();

                // Send validated shard to state management task
                if validated_tx.send((sender, message)).is_err() {
                    tracing::trace!("Validator: state management task closed, exiting");
                    break;
                }
            }

            tracing::trace!(
                "Validation task ending for publisher={}, root={}",
                publisher,
                message_root
            );
        });

        // Spawn Task 2: State Management and Reconstruction
        tokio::spawn(async move {
            tracing::trace!(
                "State management task started for publisher={}, root={}",
                publisher,
                message_root
            );

            // Message state machine
            let mut state = MessageReceivingState::Receiving { received_shards: Vec::new() };
            let mut received_count_indices = 0;
            let mut received_my_index = false;
            let mut my_shard_broadcasted = false;
            let mut reconstruction_done = false;
            let mut signature: Option<Vec<u8>> = None;

            while let Some((sender, message)) = validated_rx.recv().await {
                // Track when the first shard arrives for end-to-end latency
                tracing::trace!(
                    "State manager: processing validated shard from sender={}, index={}",
                    sender,
                    message.index()
                );

                // Update state machine
                if signature.is_none() {
                    signature = Some(message.signature().to_vec());
                }
                received_count_indices += 1;
                if message.index() == my_shard_index {
                    received_my_index = true;
                }

                let total_shards = received_count_indices
                    + if matches!(state, MessageReceivingState::Receiving { .. })
                        || received_my_index
                    {
                        0
                    } else {
                        1
                    };

                tracing::trace!(
                    "State manager: total_shards={}, received_indices={}, my_shard_broadcasted={}",
                    total_shards,
                    received_count_indices,
                    my_shard_broadcasted
                );

                match &mut state {
                    MessageReceivingState::Receiving { received_shards } => {
                        received_shards.push(message.clone());

                        // Check if this is my shard
                        if message.index() == my_shard_index && !my_shard_broadcasted {
                            tracing::trace!(
                                "State manager: received MY shard (index={}), broadcasting to \
                                 peers",
                                message.index()
                            );
                            my_shard_broadcasted = true;

                            let broadcast_to: Vec<PeerId> = tree_manager_state
                                .get_nodes()
                                .iter()
                                .map(|(peer, _)| *peer)
                                .filter(|peer| *peer != publisher && *peer != local_peer_id)
                                .collect();

                            tracing::trace!(
                                "State manager: broadcasting my shard to {} peers",
                                broadcast_to.len()
                            );

                            let _ = result_tx_state.send(ProcessorResult::BroadcastMyShard {
                                channel,
                                publisher,
                                message_root,
                                my_shard: message.clone(),
                                broadcast_to,
                            });
                            waker_notifier_state.wake();
                        }

                        // Check if we can reconstruct
                        tracing::trace!(
                            "State manager: checking reconstruction: should_build({})={}, \
                             reconstruction_done={}, my_shard_broadcasted={}",
                            total_shards,
                            tree_manager_state.should_build(total_shards),
                            reconstruction_done,
                            my_shard_broadcasted
                        );

                        if tree_manager_state.should_build(total_shards) && !reconstruction_done {
                            tracing::trace!("State manager: starting reconstruction");
                            reconstruction_done = true;

                            // Reed-Solomon reconstruction (CPU-intensive, using Rayon)
                            let shards = received_shards.clone();
                            let sig = signature.clone();
                            let reconstruction_start = std::time::Instant::now();
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            rayon::spawn({
                                let config = config_state.clone();
                                let tree_manager = Arc::clone(&tree_manager_state);

                                move || {
                                    let result: Result<
                                        (Vec<u8>, PropellerUnit, Vec<PeerId>),
                                        ReconstructionError,
                                    > = (|| {
                                        let (message, my_shard, proof) = Self::re_build_message(
                                            shards,
                                            message_root,
                                            my_shard_index.0.try_into().unwrap(),
                                            data_shards,
                                            coding_shards,
                                        )?;

                                        let un_padded_message = if config.pad() {
                                            Self::un_pad_message(message)?
                                        } else {
                                            message
                                        };

                                        let broadcast_to: Vec<PeerId> = tree_manager
                                            .get_nodes()
                                            .iter()
                                            .map(|(peer, _)| *peer)
                                            .filter(|peer| {
                                                *peer != publisher && *peer != local_peer_id
                                            })
                                            .collect();

                                        let my_shard_message = PropellerUnit::new(
                                            channel,
                                            publisher,
                                            message_root,
                                            sig.expect("Signature should be set"),
                                            my_shard_index,
                                            my_shard,
                                            proof,
                                        );

                                        Ok((un_padded_message, my_shard_message, broadcast_to))
                                    })();
                                    let _ = tx.send(result);
                                }
                            });
                            let reconstruction_result = rx.await.unwrap();
                            let reconstruction_duration = reconstruction_start.elapsed();

                            // Record reconstruction timing
                            if let Some(ref metrics) = metrics_state {
                                metrics.record_reconstruction_duration(reconstruction_duration);
                            }

                            match reconstruction_result {
                                Ok((message_data, my_shard_msg, broadcast_to)) => {
                                    tracing::trace!("State manager: reconstruction successful");

                                    // Only broadcast if we haven't already broadcast our shard
                                    if !my_shard_broadcasted {
                                        tracing::trace!(
                                            "State manager: broadcasting reconstructed shard to \
                                             {} peers",
                                            broadcast_to.len()
                                        );
                                        my_shard_broadcasted = true;
                                        let _ = result_tx_state.send(
                                            ProcessorResult::BroadcastMyShard {
                                                channel,
                                                publisher,
                                                message_root,
                                                my_shard: my_shard_msg,
                                                broadcast_to,
                                            },
                                        );
                                        waker_notifier_state.wake();
                                    }

                                    // Check if should emit reconstructed message
                                    let should_receive =
                                        tree_manager_state.should_receive(total_shards);
                                    tracing::trace!(
                                        "State manager: should_receive({})={}",
                                        total_shards,
                                        should_receive
                                    );

                                    if should_receive {
                                        tracing::trace!(
                                            "State manager: emitting reconstructed message \
                                             (len={})",
                                            message_data.len()
                                        );

                                        // Record end-to-end latency
                                        if let Some(ref metrics) = metrics_state {
                                            metrics.record_end_to_end_latency(start_time.elapsed());
                                        }

                                        let _ = result_tx_state.send(
                                            ProcessorResult::MessageReconstructed {
                                                channel,
                                                publisher,
                                                message_root,
                                                message: message_data,
                                            },
                                        );
                                        waker_notifier_state.wake();

                                        return; // Done with this message
                                    } else {
                                        state = MessageReceivingState::Built {
                                            un_padded_message: message_data,
                                        };
                                    }
                                }
                                Err(error) => {
                                    let _ = result_tx_state.send(
                                        ProcessorResult::ReconstructionFailed {
                                            channel,
                                            publisher,
                                            message_root,
                                            error,
                                        },
                                    );
                                    waker_notifier_state.wake();
                                    return; // Terminate processor
                                }
                            }
                        }
                    }

                    MessageReceivingState::Built { un_padded_message } => {
                        // Already reconstructed, waiting for threshold
                        if tree_manager_state.should_receive(total_shards) {
                            let message_data = std::mem::take(un_padded_message);

                            // Record end-to-end latency
                            if let Some(ref metrics) = metrics_state {
                                metrics.record_end_to_end_latency(start_time.elapsed());
                            }

                            let _ = result_tx_state.send(ProcessorResult::MessageReconstructed {
                                channel,
                                publisher,
                                message_root,
                                message: message_data,
                            });
                            waker_notifier_state.wake();

                            return; // Done
                        }
                    }
                }
            }

            tracing::trace!(
                "State management task ending for publisher={}, root={}",
                publisher,
                message_root
            );
        });

        tx
    }

    fn emit_event(&mut self, event: Event) {
        self.events.push_back(ToSwarm::GenerateEvent(event));
        self.update_collection_metrics();
        self.waker_notifier.wake();
    }

    fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains_key(&peer_id) {
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

        let connections = self.connected_peers.get(&peer_id).unwrap();
        let random_connection = connections.choose(&mut rand::thread_rng()).unwrap();
        self.events.push_back(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(*random_connection),
            event,
        });
        self.update_collection_metrics();
        self.waker_notifier.wake();
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
            self.config.max_wire_message_size(),
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
            self.config.max_wire_message_size(),
            self.config.substream_timeout(),
        ))
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint: _,
                failed_addresses: _,
                other_established: _,
            }) => {
                self.connected_peers.entry(peer_id).or_default().push(connection_id);

                // Update connected peers metric
                if let Some(metrics) = &self.metrics {
                    metrics.update_collection_length(
                        crate::metrics::CollectionLabel::ConnectedPeers,
                        self.connected_peers.len(),
                    );
                }
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                endpoint: _,
                remaining_established,
                cause: _,
            }) => {
                // Remove the connection ID from tracking
                if let Some(connections) = self.connected_peers.get_mut(&peer_id) {
                    connections.retain(|&id| id != connection_id);
                }

                if remaining_established == 0 {
                    self.connected_peers.remove(&peer_id);

                    // Update connected peers metric
                    if let Some(metrics) = &self.metrics {
                        metrics.update_collection_length(
                            crate::metrics::CollectionLabel::ConnectedPeers,
                            self.connected_peers.len(),
                        );
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
            HandlerOut::Unit(message) => {
                tracing::trace!(
                    "on_connection_handler_event: received shard from peer={}, channel={}, \
                     publisher={}, root={}, index={}",
                    peer_id,
                    message.channel(),
                    message.publisher(),
                    message.root(),
                    message.index()
                );

                // Track received shard
                if let Some(metrics) = &self.metrics {
                    metrics.shards_received.increment(1);
                    metrics.shard_bytes_received.increment(Self::to_u64(message.shard().len()));
                }

                // Check if channel is registered
                if !self.channel_manager.is_channel_registered(&message.channel()) {
                    tracing::warn!(
                        "Received shard for unregistered channel={}, dropping",
                        message.channel()
                    );
                    return;
                }

                if self.channel_manager.is_message_finalized(
                    &message.channel(),
                    &message.publisher(),
                    &message.root(),
                ) {
                    tracing::trace!(
                        "Received shard for finalized message, channel={}, publisher={}, root={}",
                        message.channel(),
                        message.publisher(),
                        message.root()
                    );
                    return;
                }

                // FAST PATH: Dispatch to parallel processor
                // Get or create processor for this message
                let channel = message.channel();
                let publisher = message.publisher();
                let root = message.root();

                // Check if processor exists, if not create it
                if !self.channel_manager.has_processor(&channel, &publisher, &root) {
                    tracing::trace!(
                        "Spawning new processor for channel={}, publisher={}, root={}",
                        channel,
                        publisher,
                        root
                    );
                    let tx = self.spawn_message_processor(channel, publisher, root);
                    self.channel_manager.insert_processor(channel, publisher, root, tx);
                }

                // Get the processor and send the shard
                if let Some(handle) =
                    self.channel_manager.get_processor(&channel, &publisher, &root)
                {
                    // Send shard to processor (non-blocking, < 1μs)
                    if let Err(e) = handle.tx.send((peer_id, message)) {
                        tracing::error!("Failed to send shard to processor: {:?}", e);
                    }
                    tracing::trace!("on_connection_handler_event: dispatched shard to processor");
                }
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
        self.waker_notifier.set_waker(cx.waker().clone());

        tracing::trace!(
            "poll: active_processors={}, events_queue={}",
            self.channel_manager.total_active_processors(),
            self.events.len()
        );

        while let Ok(result) = self.broadcaster_results_rx.try_recv() {
            match result {
                Ok(units) => {
                    if let Err(error) = self.broadcast_prepared_units(units) {
                        self.emit_event(Event::ShardPublishFailed { error });
                    }
                }
                Err(error) => {
                    self.emit_event(Event::ShardPublishFailed { error });
                }
            }
        }

        // Process results from parallel message processors (very fast - just moving data)
        let mut results_processed = 0;
        while let Ok(result) = self.processor_results_rx.try_recv() {
            results_processed += 1;
            tracing::trace!("poll: processing result #{}", results_processed);

            match result {
                ProcessorResult::ShardValidated {
                    channel,
                    sender,
                    publisher,
                    message_root,
                    shard_index,
                } => {
                    tracing::trace!(
                        "poll: ShardValidated from sender={}, channel={}, publisher={}, root={}, \
                         index={}",
                        sender,
                        channel,
                        publisher,
                        message_root,
                        shard_index
                    );

                    // Track validated shard
                    if let Some(metrics) = &self.metrics {
                        metrics.shards_validated.increment(1);
                    }
                }

                ProcessorResult::BroadcastMyShard {
                    channel,
                    publisher,
                    message_root,
                    my_shard,
                    broadcast_to,
                } => {
                    tracing::trace!(
                        "poll: BroadcastMyShard for channel={}, publisher={}, root={}, index={}, \
                         broadcast_to={} peers",
                        channel,
                        publisher,
                        message_root,
                        my_shard.index(),
                        broadcast_to.len()
                    );

                    // Track forwarded shards
                    if let Some(metrics) = &self.metrics {
                        metrics.shards_forwarded.increment(Self::to_u64(broadcast_to.len()));
                    }

                    // Fast: just queue handler events
                    self.send_unit_to_peers(my_shard, broadcast_to);

                    // Don't remove processor yet - might receive more shards
                }

                ProcessorResult::MessageReconstructed {
                    channel,
                    publisher,
                    message_root,
                    message,
                } => {
                    tracing::trace!(
                        "poll: MessageReconstructed for channel={}, publisher={}, root={}, \
                         message_len={}",
                        channel,
                        publisher,
                        message_root,
                        message.len()
                    );

                    // Track successful reconstruction
                    if let Some(metrics) = &self.metrics {
                        metrics.messages_reconstructed.increment(1);
                    }

                    // Clean up processor - message complete
                    self.channel_manager.remove_processor(&channel, &publisher, &message_root);
                    self.channel_manager.mark_message_finalized(channel, publisher, message_root);

                    // Emit event
                    self.emit_event(Event::MessageReceived { publisher, message_root, message });
                }

                ProcessorResult::ValidationFailed {
                    channel,
                    sender,
                    publisher,
                    message_root,
                    error,
                } => {
                    tracing::trace!(
                        "poll: ValidationFailed for channel={}, publisher={}, root={}, error={:?}",
                        channel,
                        publisher,
                        message_root,
                        error
                    );

                    if let Some(metrics) = &self.metrics {
                        metrics.increment_validation_failure(error.clone().into());
                    }

                    self.emit_event(Event::ShardValidationFailed {
                        sender,
                        claimed_root: message_root,
                        claimed_publisher: publisher,
                        error,
                    });
                }

                ProcessorResult::ReconstructionFailed {
                    channel,
                    publisher,
                    message_root,
                    error,
                } => {
                    tracing::trace!(
                        "poll: ReconstructionFailed for channel={}, publisher={}, root={}, \
                         error={:?}",
                        channel,
                        publisher,
                        message_root,
                        error
                    );

                    // Track failed reconstruction
                    if let Some(metrics) = &self.metrics {
                        metrics.messages_reconstruction_failed.increment(1);
                    }

                    // Clean up processor - reconstruction failed
                    self.channel_manager.remove_processor(&channel, &publisher, &message_root);
                    self.channel_manager.mark_message_finalized(channel, publisher, message_root);

                    self.emit_event(Event::MessageReconstructionFailed {
                        publisher,
                        message_root,
                        error,
                    });
                }
            }
        }

        if results_processed > 0 {
            tracing::trace!("poll: processed {} results from processors", results_processed);
        }

        // Update all collection metrics
        self.update_collection_metrics();

        // Return any pending events
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}
