//! Propeller core logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, channel
//! management). It implements `futures::Stream` and is polled by the libp2p `NetworkBehaviour`
//! adapter in `behaviour.rs`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{Keypair, PeerId, PublicKey};
use lru_time_cache::LruCache;
use tokio::sync::{mpsc, oneshot};

use crate::channel_utils::{send_critical, send_non_critical, ChannelName};
use crate::config::Config;
use crate::deadline_wrapper::spawn_monitored;
use crate::handler::HandlerIn;
use crate::metrics::{CollectionLabel, PropellerMetrics, ShardSendFailureReason};
use crate::reed_solomon::{generate_coding_shards, split_data_into_shards};
use crate::tasks::{
    spawn_state_manager_task,
    spawn_validator_task,
    StateManagerToCore,
    UnitToValidate,
};
use crate::tree::PropellerTreeManager;
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardIndex, ShardPublishError};
use crate::unit::PropellerUnit;
use crate::unit_validator::UnitValidator;
use crate::{signature, MerkleTree, MessageAuthenticity};

/// Commands sent from Behaviour to Core.
pub(crate) enum CoreCommand {
    RegisterChannelPeers {
        channel: Channel,
        peers: Vec<(PeerId, u64, Option<PublicKey>)>,
        response: oneshot::Sender<Result<(), PeerSetError>>,
    },
    PeerCount {
        channel: Channel,
        response: oneshot::Sender<Option<usize>>,
    },
    RegisteredChannels {
        response: oneshot::Sender<Vec<Channel>>,
    },
    Broadcast {
        channel: Channel,
        message: Vec<u8>,
        response: oneshot::Sender<Result<MessageRoot, ShardPublishError>>,
    },
    HandleUnit {
        sender: PeerId,
        unit: PropellerUnit,
    },
    HandleConnected {
        peer_id: PeerId,
    },
    HandleDisconnected {
        peer_id: PeerId,
    },
    HandleSendError {
        peer_id: PeerId,
        error: String,
    },
    UpdateEventsQueueLen {
        len: usize,
    },
}

/// Outputs emitted by the core (polled by Behaviour).
pub(crate) enum CoreOutput {
    GenerateEvent(Event),
    NotifyHandler { peer_id: PeerId, event: HandlerIn },
}

/// Handle for per-message validator task (only stores the sender channel).
struct MessageTaskHandle {
    validator_tx: mpsc::Sender<UnitToValidate>,
}

/// Data associated with a single channel.
struct ChannelData {
    tree_manager: Arc<PropellerTreeManager>,
    peer_public_keys: HashMap<PeerId, PublicKey>,
    /// Recently finalized message IDs (for deduplication). Stores only keys, not full state.
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
            finalized_messages: LruCache::with_expiry_duration(finalized_message_ttl),
        }
    }
}

/// Manages all channels and their associated data.
struct ChannelManager {
    local_peer_id: PeerId,
    finalized_message_ttl: Duration,
    channels: HashMap<Channel, ChannelData>,
}

impl ChannelManager {
    fn new(local_peer_id: PeerId, finalized_message_ttl: Duration) -> Self {
        Self { local_peer_id, finalized_message_ttl, channels: HashMap::new() }
    }

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

    fn get_tree_manager(&self, channel: &Channel) -> Option<&Arc<PropellerTreeManager>> {
        self.channels.get(channel).map(|data| &data.tree_manager)
    }

    fn get_peer_public_keys(&self, channel: &Channel) -> Option<&HashMap<PeerId, PublicKey>> {
        self.channels.get(channel).map(|data| &data.peer_public_keys)
    }

    fn is_channel_registered(&self, channel: &Channel) -> bool {
        self.channels.contains_key(channel)
    }

    fn registered_channels(&self) -> Vec<Channel> {
        self.channels.keys().copied().collect()
    }

    fn peer_count(&self, channel: &Channel) -> Option<usize> {
        self.get_tree_manager(channel).map(|tm| tm.get_node_count())
    }

    fn is_message_finalized(
        &mut self,
        channel: &Channel,
        publisher: &PeerId,
        root: &MessageRoot,
    ) -> bool {
        self.channels
            .get_mut(channel)
            .map(|data| data.finalized_messages.get(&(*publisher, *root)).is_some())
            .unwrap_or(false)
    }

    fn mark_message_finalized(&mut self, channel: Channel, publisher: PeerId, root: MessageRoot) {
        if let Some(data) = self.channels.get_mut(&channel) {
            // Add to finalized cache (just the key, no state)
            data.finalized_messages.insert((publisher, root), ());
        }
    }

    fn total_finalized_messages(&self) -> usize {
        self.channels.values().map(|data| data.finalized_messages.len()).sum()
    }

    fn num_channels(&self) -> usize {
        self.channels.len()
    }
}

/// The Propeller core (implements Stream for polling).
pub(crate) struct Core {
    config: Config,
    channel_manager: ChannelManager,
    connected_peers: HashSet<PeerId>,
    message_authenticity: MessageAuthenticity,
    local_peer_id: PeerId,

    /// Registry of per-message task handles.
    message_tasks: HashMap<(Channel, PeerId, MessageRoot), MessageTaskHandle>,

    /// Channel for receiving messages from state manager tasks.
    state_manager_rx: mpsc::Receiver<StateManagerToCore>,
    state_manager_tx: mpsc::Sender<StateManagerToCore>,

    broadcaster_results_rx: mpsc::Receiver<Result<Vec<PropellerUnit>, ShardPublishError>>,
    broadcaster_results_tx: mpsc::Sender<Result<Vec<PropellerUnit>, ShardPublishError>>,

    /// Pending outputs to be emitted.
    pending_outputs: VecDeque<CoreOutput>,

    events_queue_len: usize,
    metrics: Option<PropellerMetrics>,
}

impl Core {
    /// Create a new core instance.
    pub(crate) fn new(
        message_authenticity: MessageAuthenticity,
        config: Config,
        metrics: Option<PropellerMetrics>,
    ) -> Self {
        let local_peer_id = match &message_authenticity {
            MessageAuthenticity::Signed(keypair) => PeerId::from(keypair.public()),
            MessageAuthenticity::Author(peer_id) => *peer_id,
        };

        let (state_manager_tx, state_manager_rx) = mpsc::channel(config.channel_capacity);
        let (broadcaster_results_tx, broadcaster_results_rx) = mpsc::channel(1 << 10);

        Self {
            channel_manager: ChannelManager::new(local_peer_id, config.finalized_message_ttl),
            config,
            connected_peers: HashSet::new(),
            message_authenticity,
            local_peer_id,
            message_tasks: HashMap::new(),
            state_manager_rx,
            state_manager_tx,
            broadcaster_results_rx,
            broadcaster_results_tx,
            pending_outputs: VecDeque::new(),
            events_queue_len: 0,
            metrics,
        }
    }

    /// Register a channel with peers and optional public keys.
    pub(crate) fn register_channel_peers_and_optional_keys(
        &mut self,
        channel: Channel,
        peers: Vec<(PeerId, u64, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let mut peer_weights = Vec::new();
        let mut peer_public_keys = HashMap::new();

        for (peer_id, weight, public_key) in peers {
            match self.get_public_key(peer_id, public_key) {
                Ok(public_key) => {
                    peer_weights.push((peer_id, weight));
                    peer_public_keys.insert(peer_id, public_key);
                }
                Err(e) => return Err(e),
            }
        }

        let r = self.channel_manager.register_channel(channel, peer_weights, peer_public_keys);
        if r.is_ok() {
            if let Some(metrics) = &self.metrics {
                metrics.trees_generated.increment(1);
            }
        }

        self.update_collection_metrics();
        r
    }

    /// Get the number of peers on a channel.
    pub(crate) fn peer_count(&self, channel: Channel) -> Option<usize> {
        self.channel_manager.peer_count(&channel)
    }

    /// Get all registered channels.
    pub(crate) fn registered_channels(&self) -> Vec<Channel> {
        self.channel_manager.registered_channels()
    }

    /// Broadcast a message (returns immediately, result comes via stream).
    pub(crate) async fn broadcast(
        &mut self,
        channel: Channel,
        message: Vec<u8>,
    ) -> Result<MessageRoot, ShardPublishError> {
        // Validate channel exists.
        let Some(tree_manager) = self.channel_manager.get_tree_manager(&channel).cloned() else {
            return Err(ShardPublishError::ChannelNotRegistered(channel));
        };

        let publisher = self.local_peer_id;
        let keypair = match &self.message_authenticity {
            MessageAuthenticity::Signed(keypair) => Some(keypair.clone()),
            MessageAuthenticity::Author(_) => None,
        };

        let pad = self.config.pad;
        let num_data_shards = tree_manager.calculate_data_shards();
        let num_coding_shards = tree_manager.calculate_coding_shards();
        let tx = self.broadcaster_results_tx.clone();

        let root = spawn_monitored("prepare_units_task", async move {
            let (result_tx, result_rx) = oneshot::channel();

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
                let _ = result_tx.send(r);
            });

            let r = result_rx.await.expect("Rayon task failed to send result");

            let task_result = match &r {
                Ok(units) => Ok(units[0].root()),
                Err(error) => Err(error.clone()),
            };
            send_critical(&tx, r, ChannelName::BroadcasterToCore).await;
            task_result
        })
        .await
        .expect("Failed to join prepare_units task");

        root
    }

    /// Handle a peer connection.
    pub(crate) fn handle_connected(&mut self, peer_id: PeerId) {
        self.connected_peers.insert(peer_id);
        self.update_collection_metrics();
    }

    /// Handle a peer disconnection.
    pub(crate) fn handle_disconnected(&mut self, peer_id: PeerId) {
        self.connected_peers.remove(&peer_id);
        self.update_collection_metrics();
    }

    /// Handle an incoming unit from a peer.
    pub(crate) async fn handle_unit(&mut self, sender: PeerId, unit: PropellerUnit) {
        let channel = unit.channel();
        let publisher = unit.publisher();
        let root = unit.root();

        // Check if channel is registered.
        if !self.channel_manager.is_channel_registered(&channel) {
            tracing::warn!("Received shard for unregistered channel={}, dropping", channel);
            return;
        }

        // Track received shard.
        if let Some(metrics) = &self.metrics {
            metrics.shards_received.increment(1);
            metrics.shard_bytes_received.increment(Self::to_u64(unit.shard().len()));
        }

        // Skip if message already finalized.
        if self.channel_manager.is_message_finalized(&channel, &publisher, &root) {
            tracing::trace!("Message already finalized, dropping unit");
            return;
        }

        let message_key = (channel, publisher, root);

        // Spawn tasks if this is a new message.
        if !self.message_tasks.contains_key(&message_key) {
            tracing::trace!(
                "[CORE] Spawning tasks for new message channel={} publisher={:?} root={:?}",
                channel,
                publisher,
                root
            );

            let tree_manager = self
                .channel_manager
                .get_tree_manager(&channel)
                .expect("Channel must be registered");
            let publisher_public_key = self
                .channel_manager
                .get_peer_public_keys(&channel)
                .and_then(|keys| keys.get(&publisher))
                .cloned();
            let my_shard_index = tree_manager.get_my_shard_index(&publisher).unwrap();

            // Create channel for validator -> state manager communication
            let (validator_to_sm_tx, validator_to_sm_rx) =
                mpsc::channel(self.config.channel_capacity);

            // Spawn validator task
            let validator_handle = spawn_validator_task(
                channel,
                publisher,
                root,
                UnitValidator::new(
                    channel,
                    publisher,
                    publisher_public_key,
                    root,
                    self.config.validation_mode,
                    Arc::clone(tree_manager),
                ),
                self.config.task_timeout,
                self.config.channel_capacity,
                validator_to_sm_tx,
            );

            // Spawn state manager task
            spawn_state_manager_task(
                channel,
                publisher,
                root,
                my_shard_index,
                Arc::clone(tree_manager),
                self.local_peer_id,
                self.config.clone(),
                validator_to_sm_rx,
                validator_handle.sm_to_validator_tx,
                self.state_manager_tx.clone(),
            );

            let handle = MessageTaskHandle { validator_tx: validator_handle.unit_tx };
            self.message_tasks.insert(message_key, handle);
        }

        // Send unit to validator task
        let handle = self.message_tasks.get(&message_key).expect("Tasks must exist");
        let unit_to_validate = UnitToValidate { sender, unit };

        // this may fail if the message is already finalized
        let _ =
            send_non_critical(&handle.validator_tx, unit_to_validate, ChannelName::CoreToValidator)
                .await;

        self.update_collection_metrics();
    }

    /// Handle a send error from the handler.
    pub(crate) async fn handle_send_error(&mut self, peer_id: PeerId, error: String) {
        tokio::task::yield_now().await;

        if let Some(metrics) = &self.metrics {
            metrics.increment_send_failure(ShardSendFailureReason::HandlerError);
        }

        self.emit_event(Event::ShardSendFailed {
            sent_from: None,
            sent_to: Some(peer_id),
            error: ShardPublishError::HandlerError(error),
        })
        .await;
        self.update_collection_metrics();
    }

    /// Update the events queue length metric.
    pub(crate) fn update_events_queue_len(&mut self, len: usize) {
        self.events_queue_len = len;
        self.update_collection_metrics();
    }

    async fn emit_output(&mut self, out: CoreOutput) {
        self.pending_outputs.push_back(out);

        // Yield after emitting output
        tokio::task::yield_now().await;
    }

    async fn emit_event(&mut self, event: Event) {
        // Track metrics for this event
        if let Some(metrics) = &self.metrics {
            metrics.track_event(&event);
        }
        self.emit_output(CoreOutput::GenerateEvent(event)).await;
    }

    async fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains(&peer_id) {
            if let Some(metrics) = &self.metrics {
                metrics.increment_send_failure(ShardSendFailureReason::NotConnectedToPeer);
            }
            self.emit_event(Event::ShardSendFailed {
                sent_from: None,
                sent_to: Some(peer_id),
                error: ShardPublishError::NotConnectedToPeer(peer_id),
            })
            .await;
            return;
        }

        self.emit_output(CoreOutput::NotifyHandler { peer_id, event }).await;
    }

    fn update_collection_metrics(&self) {
        if let Some(metrics) = &self.metrics {
            metrics.update_collection_length(CollectionLabel::EventsQueue, self.events_queue_len);
            metrics.update_collection_length(
                CollectionLabel::ActiveProcessors,
                self.message_tasks.len(),
            );
            metrics.update_collection_length(
                CollectionLabel::FinalizedMessages,
                self.channel_manager.total_finalized_messages(),
            );
            metrics.update_collection_length(
                CollectionLabel::RegisteredChannels,
                self.channel_manager.num_channels(),
            );
            metrics.update_collection_length(
                CollectionLabel::ConnectedPeers,
                self.connected_peers.len(),
            );
        }
    }

    async fn handle_broadcaster_result(
        &mut self,
        result: Result<Vec<PropellerUnit>, ShardPublishError>,
    ) {
        tokio::task::yield_now().await;

        match result {
            Ok(units) => {
                if let Err(error) = self.broadcast_prepared_units(units).await {
                    self.emit_event(Event::ShardPublishFailed { error }).await;
                }
            }
            Err(error) => {
                self.emit_event(Event::ShardPublishFailed { error }).await;
            }
        }
        self.update_collection_metrics();
    }

    /// Handle messages from state manager tasks.
    async fn handle_state_manager_message(&mut self, msg: StateManagerToCore) {
        tokio::task::yield_now().await;

        match msg {
            StateManagerToCore::Event(event) => {
                self.emit_event(event).await;
            }
            StateManagerToCore::Finalized { channel, publisher, message_root } => {
                tracing::trace!(
                    "[CORE] Message finalized channel={} publisher={:?} root={:?}",
                    channel,
                    publisher,
                    message_root
                );

                // Mark as finalized in channel manager
                self.channel_manager.mark_message_finalized(channel, publisher, message_root);

                // Clean up task handles
                let message_key = (channel, publisher, message_root);
                if self.message_tasks.remove(&message_key).is_some() {
                    // Tasks will naturally terminate when they complete
                    // We could await the join handles here if we want to ensure cleanup
                    tracing::trace!(
                        "[CORE] Removed task handles for channel={} publisher={:?} root={:?}",
                        channel,
                        publisher,
                        message_root
                    );
                }
            }
            StateManagerToCore::BroadcastUnit { unit, peers } => {
                tracing::trace!(
                    "[CORE] Broadcasting unit index={:?} to {} peers (gossip)",
                    unit.index(),
                    peers.len()
                );

                // Track forwarded shards
                if let Some(metrics) = &self.metrics {
                    metrics.shards_forwarded.increment(1);
                }

                for peer in peers {
                    self.send_unit_to_peer(unit.clone(), peer).await;
                }
            }
        }

        self.update_collection_metrics();
    }

    /// Helper to convert usize to u64 safely for metrics.
    #[allow(clippy::as_conversions)]
    fn to_u64(value: usize) -> u64 {
        value as u64
    }

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

    fn pad_message(message: Vec<u8>, num_data_shards: usize) -> Vec<u8> {
        let original_message_length: u32 =
            message.len().try_into().expect("Message length too long");
        let amount_to_pad = 2 * num_data_shards - ((message.len() + 4) % (2 * num_data_shards));
        [original_message_length.to_le_bytes().to_vec(), message, vec![0; amount_to_pad]].concat()
    }

    /// Prepare units for broadcasting (pure, CPU-heavy).
    pub(crate) fn prepare_units(
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
            messages.push(message);
        }

        Ok(messages)
    }

    async fn broadcast_prepared_units(
        &mut self,
        units: Vec<PropellerUnit>,
    ) -> Result<(), ShardPublishError> {
        if units.is_empty() {
            return Ok(());
        }

        // Yield at start to prevent blocking on large broadcasts
        tokio::task::yield_now().await;

        let channel = units[0].channel();
        let publisher = self.local_peer_id;

        tracing::trace!("[BROADCAST] Publisher {:?} broadcasting {} units", publisher, units.len());

        let tree_manager = self
            .channel_manager
            .get_tree_manager(&channel)
            .ok_or(ShardPublishError::ChannelNotRegistered(channel))?;

        if let Some(metrics) = &self.metrics {
            metrics.shards_published.increment(Self::to_u64(units.len()));
        }

        let peers_in_order = tree_manager.make_first_broadcast_list();
        debug_assert_eq!(peers_in_order.len(), units.len());

        for (idx, (message, (peer, shard_index))) in
            units.into_iter().zip(peers_in_order).enumerate()
        {
            debug_assert_eq!(message.publisher(), publisher);
            debug_assert_eq!(message.channel(), channel);
            debug_assert_eq!(message.index(), shard_index);
            tracing::trace!("[BROADCAST] Sending shard index={} to peer={:?}", shard_index, peer);
            self.send_unit_to_peer(message, peer).await;

            // Yield every 5 units to prevent monopolizing runtime
            if (idx + 1) % 5 == 0 {
                tokio::task::yield_now().await;
            }
        }

        Ok(())
    }

    async fn send_unit_to_peer(&mut self, unit: PropellerUnit, peer: PeerId) {
        tokio::task::yield_now().await;
        if let Some(metrics) = &self.metrics {
            metrics.shards_sent.increment(1);
            metrics.shard_bytes_sent.increment(Self::to_u64(unit.shard().len()));
        }

        self.emit_handler_event(peer, HandlerIn::SendUnit(unit)).await;
    }

    /// Run the core in its own task, processing commands and results.
    pub(crate) async fn run(
        mut self,
        mut commands_rx: mpsc::Receiver<CoreCommand>,
        output_tx: mpsc::Sender<CoreOutput>,
    ) {
        loop {
            tokio::select! {
                // Handle commands from Behaviour
                Some(cmd) = commands_rx.recv() => {
                    self.handle_command(cmd).await;
                }

                // Process broadcaster results
                Some(result) = self.broadcaster_results_rx.recv() => {
                    self.handle_broadcaster_result(result).await;
                }

                // Process messages from state manager tasks
                Some(msg) = self.state_manager_rx.recv() => {
                    self.handle_state_manager_message(msg).await;
                }

                else => {
                    // All channels closed, exit
                    tracing::error!("Core task shutting down");
                    break;
                }
            }
            self.drain_pending_outputs(&output_tx).await;
            // tokio::task::yield_now().await;
        }
    }

    async fn handle_command(&mut self, cmd: CoreCommand) {
        tokio::task::yield_now().await;

        match cmd {
            CoreCommand::RegisterChannelPeers { channel, peers, response } => {
                let result = self.register_channel_peers_and_optional_keys(channel, peers);
                response
                    .send(result)
                    .expect("RegisterChannelPeers response channel dropped - receiver gone");
            }
            CoreCommand::PeerCount { channel, response } => {
                let result = self.peer_count(channel);
                response.send(result).expect("PeerCount response channel dropped - receiver gone");
            }
            CoreCommand::RegisteredChannels { response } => {
                let result = self.registered_channels();
                response
                    .send(result)
                    .expect("RegisteredChannels response channel dropped - receiver gone");
            }
            CoreCommand::Broadcast { channel, message, response } => {
                let result = self.broadcast(channel, message).await;
                response.send(result).expect("Broadcast response channel dropped - receiver gone");
            }
            CoreCommand::HandleUnit { sender, unit } => {
                self.handle_unit(sender, unit).await;
            }
            CoreCommand::HandleConnected { peer_id } => {
                self.handle_connected(peer_id);
            }
            CoreCommand::HandleDisconnected { peer_id } => {
                self.handle_disconnected(peer_id);
            }
            CoreCommand::HandleSendError { peer_id, error } => {
                self.handle_send_error(peer_id, error).await;
            }
            CoreCommand::UpdateEventsQueueLen { len } => {
                self.update_events_queue_len(len);
            }
        }
    }

    async fn drain_pending_outputs(&mut self, output_tx: &mpsc::Sender<CoreOutput>) {
        while let Some(output) = self.pending_outputs.pop_front() {
            tokio::task::yield_now().await;
            send_critical(output_tx, output, ChannelName::CoreToBehaviour).await;
        }
    }
}
