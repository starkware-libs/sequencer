//! Propeller engine logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, channel
//! management). It implements `futures::Stream` and is polled by the libp2p `NetworkBehaviour`
//! adapter in `behaviour.rs`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use libp2p::identity::{Keypair, PeerId, PublicKey};
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::handler::{HandlerIn, HandlerOut};
use crate::message_processor::{MessageProcessor, StateManagerToEngine, UnitToValidate};
use crate::sharding::prepare_units;
use crate::signature;
use crate::time_cache::TimeCache;
use crate::tree::PropellerScheduleManager;
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardPublishError};
use crate::unit::PropellerUnit;

/// Commands sent from Behaviour to Engine.
pub enum EngineCommand {
    RegisterChannelPeers {
        channel: Channel,
        peers: Vec<(PeerId, u64, Option<PublicKey>)>,
        response: oneshot::Sender<Result<(), PeerSetError>>,
    },
    UnregisterChannel {
        channel: Channel,
        response: oneshot::Sender<Result<(), ()>>,
    },
    Broadcast {
        channel: Channel,
        message: Vec<u8>,
        response: oneshot::Sender<Result<MessageRoot, ShardPublishError>>,
    },
    HandleHandlerOutput {
        peer_id: PeerId,
        output: HandlerOut,
    },
    HandleConnected {
        peer_id: PeerId,
    },
    HandleDisconnected {
        peer_id: PeerId,
    },
}

/// Outputs emitted by the engine (polled by Behaviour).
pub enum EngineOutput {
    GenerateEvent(Event),
    NotifyHandler { peer_id: PeerId, event: HandlerIn },
}

/// Data associated with a single channel.
// TODO(AndrewL): remove this once we use all fields.
#[allow(dead_code)]
struct ChannelData {
    tree_manager: Arc<PropellerScheduleManager>,
    peer_public_keys: HashMap<PeerId, PublicKey>,
}

/// The Propeller engine (implements Stream for polling).
// TODO(AndrewL): remove this once we use all fields.
#[allow(dead_code)]
pub struct Engine {
    config: Config,
    channels: HashMap<Channel, ChannelData>,
    connected_peers: HashSet<PeerId>,
    keypair: Keypair,
    local_peer_id: PeerId,
    /// Registry of per-message task handles.
    message_tasks: HashMap<(Channel, PeerId, MessageRoot), mpsc::UnboundedSender<UnitToValidate>>,
    /// Recently finalized message IDs (for deduplication).
    finalized_messages: TimeCache<(Channel, PeerId, MessageRoot)>,
    /// Channel for receiving messages from state manager tasks.
    state_manager_rx: mpsc::UnboundedReceiver<StateManagerToEngine>,
    state_manager_tx: mpsc::UnboundedSender<StateManagerToEngine>,
    broadcaster_results_rx: mpsc::UnboundedReceiver<Result<Vec<PropellerUnit>, ShardPublishError>>,
    broadcaster_results_tx: mpsc::UnboundedSender<Result<Vec<PropellerUnit>, ShardPublishError>>,
    output_tx: mpsc::UnboundedSender<EngineOutput>,
}

impl Engine {
    /// Create a new engine instance.
    pub fn new(
        keypair: Keypair,
        config: Config,
        output_tx: mpsc::UnboundedSender<EngineOutput>,
    ) -> Self {
        let local_peer_id = PeerId::from(keypair.public());
        let (state_manager_tx, state_manager_rx) = mpsc::unbounded_channel();
        let (broadcaster_results_tx, broadcaster_results_rx) = mpsc::unbounded_channel();

        Self {
            channels: HashMap::new(),
            config: config.clone(),
            connected_peers: HashSet::new(),
            keypair,
            local_peer_id,
            message_tasks: HashMap::new(),
            finalized_messages: TimeCache::new(config.stale_message_timeout),
            state_manager_rx,
            state_manager_tx,
            broadcaster_results_rx,
            broadcaster_results_tx,
            output_tx,
        }
    }

    /// Register a channel with peers and optional public keys.
    pub fn register_channel_peers_and_optional_keys(
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

        let new_tree_manager = PropellerScheduleManager::new(self.local_peer_id, peer_weights)?;
        let channel_data =
            ChannelData { tree_manager: Arc::new(new_tree_manager), peer_public_keys };
        self.channels.insert(channel, channel_data);

        Ok(())
    }

    /// Unregister a channel.
    #[allow(clippy::result_unit_err)] // TODO(AndrewL): remove this
    pub fn unregister_channel(&mut self, channel: Channel) -> Result<(), ()> {
        self.channels.remove(&channel).ok_or(())?;
        Ok(())
    }

    /// Broadcast a message (returns immediately, result comes via stream).
    pub(crate) async fn broadcast(
        &mut self,
        channel: Channel,
        message: Vec<u8>,
    ) -> Result<MessageRoot, ShardPublishError> {
        // Validate channel exists.
        let Some(tree_manager) = self.channels.get(&channel).map(|data| data.tree_manager.clone())
        else {
            return Err(ShardPublishError::ChannelNotRegistered(channel));
        };

        let publisher = self.local_peer_id;
        let keypair = self.keypair.clone();

        let num_data_shards = tree_manager.num_data_shards();
        let num_coding_shards = tree_manager.num_coding_shards();
        let tx = self.broadcaster_results_tx.clone();

        let root = tokio::spawn(async move {
            let (result_tx, result_rx) = oneshot::channel();

            rayon::spawn(move || {
                let r = prepare_units(
                    channel,
                    publisher,
                    keypair,
                    message,
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
            tx.send(r).expect("Engine task has exited");
            task_result
        })
        .await
        .expect("Failed to join prepare_units task");

        root
    }

    /// Handle a peer connection.
    pub(crate) fn handle_connected(&mut self, peer_id: PeerId) {
        self.connected_peers.insert(peer_id);
    }

    /// Handle a peer disconnection.
    pub(crate) fn handle_disconnected(&mut self, peer_id: PeerId) {
        self.connected_peers.remove(&peer_id);
    }

    /// Handle an incoming unit from a peer.
    pub(crate) async fn handle_unit(&mut self, sender: PeerId, unit: PropellerUnit) {
        let channel = unit.channel();
        let publisher = unit.publisher();
        let root = unit.root();

        // Check if channel is registered.
        if !self.channels.contains_key(&channel) {
            tracing::warn!("Received shard for unregistered channel={:?}, dropping", channel);
            return;
        }

        // Skip if message already finalized.
        if self.finalized_messages.contains(&(channel, publisher, root)) {
            tracing::trace!("Message already finalized, dropping unit");
            return;
        }

        let message_key = (channel, publisher, root);

        // Spawn tasks if this is a new message.
        if !self.message_tasks.contains_key(&message_key) {
            tracing::trace!(
                "[ENGINE] Spawning tasks for new message channel={:?} publisher={:?} root={:?}",
                channel,
                publisher,
                root
            );

            let tree_manager = self
                .channels
                .get(&channel)
                .expect("Channel must be registered")
                .tree_manager
                .clone();
            let publisher_public_key = self
                .channels
                .get(&channel)
                .and_then(|data| data.peer_public_keys.get(&publisher))
                .cloned()
                .expect("Publisher must have a public key");
            let my_shard_index = tree_manager.get_my_shard_index(&publisher).unwrap();

            // Create channel for Engine -> MessageProcessor communication
            let (unit_tx, unit_rx) = mpsc::unbounded_channel();

            // Create and spawn message processor
            let processor = MessageProcessor {
                channel,
                publisher,
                message_root: root,
                my_shard_index,
                publisher_public_key,
                tree_manager: Arc::clone(&tree_manager),
                local_peer_id: self.local_peer_id,
                unit_rx,
                engine_tx: self.state_manager_tx.clone(),
                timeout: self.config.stale_message_timeout,
            };

            tokio::spawn(processor.run());

            self.message_tasks.insert(message_key, unit_tx);
        }

        // Send unit to message processor
        let handle = self.message_tasks.get(&message_key).expect("Message processor must exist");

        // This may fail if the message is already finalized
        let _ = handle.send((sender, unit));
    }

    /// Handle a send error from the handler.
    pub(crate) async fn handle_send_error(&mut self, peer_id: PeerId, error: String) {
        self.emit_event(Event::ShardSendFailed {
            sent_from: None,
            sent_to: Some(peer_id),
            error: ShardPublishError::HandlerError(error),
        })
        .await;
    }

    async fn emit_output(&mut self, out: EngineOutput) {
        self.output_tx.send(out).expect("Behaviour has exited");
    }

    async fn emit_event(&mut self, event: Event) {
        self.emit_output(EngineOutput::GenerateEvent(event)).await;
    }

    async fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains(&peer_id) {
            self.emit_event(Event::ShardSendFailed {
                sent_from: None,
                sent_to: Some(peer_id),
                error: ShardPublishError::NotConnectedToPeer(peer_id),
            })
            .await;
            return;
        }

        self.emit_output(EngineOutput::NotifyHandler { peer_id, event }).await;
    }

    async fn handle_broadcaster_result(
        &mut self,
        result: Result<Vec<PropellerUnit>, ShardPublishError>,
    ) {
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

    async fn broadcast_prepared_units(
        &mut self,
        units: Vec<PropellerUnit>,
    ) -> Result<(), ShardPublishError> {
        if units.is_empty() {
            return Ok(());
        }

        let channel = units[0].channel();
        let publisher = self.local_peer_id;

        tracing::trace!("[BROADCAST] Publisher {:?} broadcasting {} units", publisher, units.len());

        let tree_manager = self
            .channels
            .get(&channel)
            .ok_or(ShardPublishError::ChannelNotRegistered(channel))?
            .tree_manager
            .clone();

        let peers_in_order = tree_manager.make_broadcast_list();
        debug_assert_eq!(peers_in_order.len(), units.len());

        for (unit, peer) in units.into_iter().zip(peers_in_order) {
            tracing::trace!(
                "[BROADCAST] Sending shard index={:?} to peer={:?}",
                unit.index(),
                peer
            );
            self.send_unit_to_peer(unit, peer).await;
        }

        Ok(())
    }

    async fn send_unit_to_peer(&mut self, unit: PropellerUnit, peer: PeerId) {
        self.emit_handler_event(peer, HandlerIn::SendUnit(unit)).await;
    }

    /// Run the engine in its own task, processing commands and results.
    pub async fn run(mut self, mut commands_rx: mpsc::UnboundedReceiver<EngineCommand>) {
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

                else => {
                    // All channels closed, exit
                    tracing::error!("Engine task shutting down");
                    break;
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::RegisterChannelPeers { channel, peers, response } => {
                let result = self.register_channel_peers_and_optional_keys(channel, peers);
                response
                    .send(result)
                    .expect("RegisterChannelPeers response channel dropped - receiver gone");
            }
            EngineCommand::UnregisterChannel { channel, response } => {
                let result = self.unregister_channel(channel);
                response
                    .send(result)
                    .expect("UnregisterChannel response channel dropped - receiver gone");
            }
            EngineCommand::Broadcast { channel, message, response } => {
                let result = self.broadcast(channel, message).await;
                response.send(result).expect("Broadcast response channel dropped - receiver gone");
            }
            EngineCommand::HandleHandlerOutput { peer_id, output } => match output {
                HandlerOut::Unit(unit) => {
                    self.handle_unit(peer_id, unit).await;
                }
                HandlerOut::SendError(error) => {
                    self.handle_send_error(peer_id, error).await;
                }
            },
            EngineCommand::HandleConnected { peer_id } => {
                self.handle_connected(peer_id);
            }
            EngineCommand::HandleDisconnected { peer_id } => {
                self.handle_disconnected(peer_id);
            }
        }
    }
}
