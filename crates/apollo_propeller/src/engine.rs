//! Propeller engine logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, channel
//! management). The engine runs as an async task and communicates with the libp2p
//! `NetworkBehaviour` adapter in `behaviour.rs` via command/output channels.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use libp2p::identity::{Keypair, PeerId, PublicKey};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace, warn};

use crate::config::Config;
use crate::handler::{HandlerIn, HandlerOut};
use crate::message_processor::{EventStateManagerToEngine, MessageProcessor, UnitToValidate};
use crate::sharding::create_units_to_publish;
use crate::signature;
use crate::time_cache::TimeCache;
use crate::tree::{PropellerScheduleManager, Stake};
use crate::types::{Channel, Event, MessageRoot, PeerSetError, ShardPublishError};
use crate::unit::PropellerUnit;

type BroadcastResponse = oneshot::Sender<Result<(), ShardPublishError>>;
type BroadcastResult = (Result<Vec<PropellerUnit>, ShardPublishError>, BroadcastResponse);

/// Commands sent from Behaviour to Engine.
pub enum EngineCommand {
    RegisterChannelPeers {
        channel: Channel,
        peers: Vec<(PeerId, Stake, Option<PublicKey>)>,
        response: oneshot::Sender<Result<(), PeerSetError>>,
    },
    // TODO(AndrewL): remove this variant once unregister is no longer needed.
    UnregisterChannel {
        channel: Channel,
        response: oneshot::Sender<bool>,
    },
    Broadcast {
        channel: Channel,
        message: Vec<u8>,
        response: BroadcastResponse,
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

/// Outputs sent from the engine to the Behaviour.
pub enum EngineOutput {
    GenerateEvent(Event),
    NotifyHandler { peer_id: PeerId, event: HandlerIn },
}

/// Data associated with a single channel.
// TODO(AndrewL): rename to CommitteeData when channel is renamed to committee.
struct ChannelData {
    tree_manager: Arc<PropellerScheduleManager>,
    peer_public_keys: HashMap<PeerId, PublicKey>,
}

/// The Propeller engine, run as an async task via [`Engine::run`].
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
    state_manager_rx: mpsc::UnboundedReceiver<EventStateManagerToEngine>,
    state_manager_tx: mpsc::UnboundedSender<EventStateManagerToEngine>,
    broadcaster_results_rx: mpsc::UnboundedReceiver<BroadcastResult>,
    broadcaster_results_tx: mpsc::UnboundedSender<BroadcastResult>,
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

        let finalized_messages = TimeCache::new(config.stale_message_timeout);

        Self {
            channels: HashMap::new(),
            config,
            connected_peers: HashSet::new(),
            keypair,
            local_peer_id,
            message_tasks: HashMap::new(),
            finalized_messages,
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
        peers: Vec<(PeerId, Stake, Option<PublicKey>)>,
    ) -> Result<(), PeerSetError> {
        let mut peer_weights = Vec::new();
        let mut peer_public_keys = HashMap::new();

        for (peer_id, weight, public_key) in peers {
            let public_key = self.get_public_key(peer_id, public_key)?;
            peer_weights.push((peer_id, weight));
            peer_public_keys.insert(peer_id, public_key);
        }

        let new_tree_manager = PropellerScheduleManager::new(self.local_peer_id, peer_weights)?;
        let channel_data =
            ChannelData { tree_manager: Arc::new(new_tree_manager), peer_public_keys };
        self.channels.insert(channel, channel_data);

        Ok(())
    }

    /// Unregister a channel.
    // TODO(AndrewL): clean up message_tasks entries and terminate their processor tasks on
    // unregister to avoid resource leaks.
    pub fn unregister_channel(&mut self, channel: Channel) -> bool {
        self.channels.remove(&channel).is_some()
    }

    /// Broadcast a message on a channel.
    ///
    /// The result is delivered asynchronously via `response` after the units are created and
    /// distributed; the caller does not block on sharding.
    fn broadcast(&mut self, channel: Channel, message: Vec<u8>, response: BroadcastResponse) {
        let Some(tree_manager) = self.channels.get(&channel).map(|data| data.tree_manager.clone())
        else {
            let _ = response.send(Err(ShardPublishError::ChannelNotRegistered(channel)));
            return;
        };

        let keypair = self.keypair.clone();
        let num_data_shards = tree_manager.num_data_shards();
        let num_coding_shards = tree_manager.num_coding_shards();
        let tx = self.broadcaster_results_tx.clone();

        tokio::task::spawn_blocking(move || {
            let result = create_units_to_publish(
                message,
                channel,
                keypair,
                num_data_shards,
                num_coding_shards,
            );
            let _ = tx.send((result, response));
        });
    }

    /// Handle a peer connection.
    fn handle_connected(&mut self, peer_id: PeerId) {
        self.connected_peers.insert(peer_id);
    }

    /// Handle a peer disconnection.
    fn handle_disconnected(&mut self, peer_id: PeerId) {
        self.connected_peers.remove(&peer_id);
    }

    /// Handle an incoming unit from a peer.
    fn handle_unit(&mut self, sender: PeerId, unit: PropellerUnit) {
        let channel = unit.channel();
        let publisher = unit.publisher();
        let root = unit.root();

        // Check if channel is registered.
        if !self.channels.contains_key(&channel) {
            warn!(?channel, "Received shard for unregistered channel, dropping");
            return;
        }

        // Skip if message already finalized.
        if self.finalized_messages.contains(&(channel, publisher, root)) {
            trace!("Message already finalized, dropping unit");
            return;
        }

        let message_key = (channel, publisher, root);

        // Spawn tasks if this is a new message.
        if !self.message_tasks.contains_key(&message_key) {
            trace!(?channel, ?publisher, ?root, "[ENGINE] Spawning new message processor");

            let channel_data = self.channels.get(&channel).expect("Channel must be registered");
            let tree_manager = channel_data.tree_manager.clone();
            let publisher_public_key = channel_data
                .peer_public_keys
                .get(&publisher)
                .cloned()
                .expect("Publisher must have a public key");
            let my_shard_index =
                tree_manager.get_my_shard_index_given_publisher(&publisher).unwrap();

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
    fn handle_send_error(&mut self, peer_id: PeerId, error: String) {
        self.emit_event(Event::ShardSendFailed {
            sent_from: None,
            sent_to: Some(peer_id),
            error: ShardPublishError::HandlerError(error),
        });
    }

    fn emit_event(&mut self, event: Event) {
        self.output_tx.send(EngineOutput::GenerateEvent(event)).expect("Behaviour has exited");
    }

    fn emit_handler_event(&mut self, peer_id: PeerId, event: HandlerIn) {
        if !self.connected_peers.contains(&peer_id) {
            self.emit_event(Event::ShardSendFailed {
                sent_from: None,
                sent_to: Some(peer_id),
                error: ShardPublishError::NotConnectedToPeer(peer_id),
            });
            return;
        }
        self.output_tx
            .send(EngineOutput::NotifyHandler { peer_id, event })
            .expect("Behaviour has exited");
    }

    fn handle_broadcaster_result(
        &mut self,
        result: Result<Vec<PropellerUnit>, ShardPublishError>,
        response: BroadcastResponse,
    ) {
        if let Err(error) = result.and_then(|units| self.broadcast_prepared_units(units)) {
            let _ = response.send(Err(error));
        } else {
            let _ = response.send(Ok(()));
        }
    }

    /// Handle messages from state manager tasks.
    fn handle_state_manager_message(&mut self, msg: EventStateManagerToEngine) {
        match msg {
            EventStateManagerToEngine::BehaviourEvent(event) => {
                self.emit_event(event);
            }
            EventStateManagerToEngine::Finalized { channel, publisher, message_root } => {
                trace!(?channel, ?publisher, ?message_root, "[ENGINE] Message finalized");

                // Mark as finalized
                self.finalized_messages.insert((channel, publisher, message_root));

                // Clean up task handles
                let message_key = (channel, publisher, message_root);
                if self.message_tasks.remove(&message_key).is_some() {
                    trace!(?channel, ?publisher, ?message_root, "[ENGINE] Removed task handles");
                }
            }
            EventStateManagerToEngine::SendUnitToPeers { unit, peers } => {
                trace!(index = ?unit.index(), num_peers = peers.len(), "[ENGINE] Forwarding unit to peers");

                for peer in peers {
                    self.send_unit_to_peer(unit.clone(), peer);
                }
            }
        }
    }

    fn get_public_key(
        &self,
        peer_id: PeerId,
        public_key: Option<PublicKey>,
    ) -> Result<PublicKey, PeerSetError> {
        match public_key {
            Some(pk) if signature::validate_public_key_matches_peer_id(&pk, &peer_id) => Ok(pk),
            Some(_) => Err(PeerSetError::InvalidPublicKey),
            None => signature::try_extract_public_key_from_peer_id(&peer_id)
                .ok_or(PeerSetError::InvalidPublicKey),
        }
    }

    fn broadcast_prepared_units(
        &mut self,
        units: Vec<PropellerUnit>,
    ) -> Result<(), ShardPublishError> {
        if units.is_empty() {
            return Ok(());
        }

        let channel = units[0].channel();
        trace!(publisher = ?self.local_peer_id, num_units = units.len(), "[BROADCAST] Broadcasting units");

        let tree_manager = self
            .channels
            .get(&channel)
            .ok_or(ShardPublishError::ChannelNotRegistered(channel))?
            .tree_manager
            .clone();

        let peers_in_order = tree_manager.make_broadcast_list();
        debug_assert_eq!(peers_in_order.len(), units.len());

        for (unit, peer) in units.into_iter().zip(peers_in_order) {
            trace!(index = ?unit.index(), ?peer, "[BROADCAST] Sending shard");
            self.send_unit_to_peer(unit, peer);
        }

        Ok(())
    }

    fn send_unit_to_peer(&mut self, unit: PropellerUnit, peer: PeerId) {
        self.emit_handler_event(peer, HandlerIn::SendUnit(unit));
    }

    /// Run the engine in its own task, processing commands and results.
    pub async fn run(mut self, mut commands_rx: mpsc::UnboundedReceiver<EngineCommand>) {
        loop {
            tokio::select! {
                Some(cmd) = commands_rx.recv() => match cmd {
                    EngineCommand::RegisterChannelPeers { channel, peers, response } => {
                        let result = self.register_channel_peers_and_optional_keys(channel, peers);
                        let _ = response.send(result);
                    }
                    EngineCommand::UnregisterChannel { channel, response } => {
                        let _ = response.send(self.unregister_channel(channel));
                    }
                    EngineCommand::Broadcast { channel, message, response } => {
                        self.broadcast(channel, message, response);
                    }
                    EngineCommand::HandleHandlerOutput { peer_id, output } => match output {
                        HandlerOut::Unit(unit) => self.handle_unit(peer_id, unit),
                        HandlerOut::SendError(error) => self.handle_send_error(peer_id, error),
                    },
                    EngineCommand::HandleConnected { peer_id } => self.handle_connected(peer_id),
                    EngineCommand::HandleDisconnected { peer_id } => self.handle_disconnected(peer_id),
                },

                Some((result, response)) = self.broadcaster_results_rx.recv() => {
                    self.handle_broadcaster_result(result, response);
                }

                Some(msg) = self.state_manager_rx.recv() => {
                    self.handle_state_manager_message(msg);
                }

                else => {
                    error!("Engine task shutting down");
                    break;
                }
            }
        }
    }
}
