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
use crate::message_processor::{StateManagerToEngine, UnitToValidate};
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

    /// Handle a peer connection.
    pub(crate) fn handle_connected(&mut self, peer_id: PeerId) {
        self.connected_peers.insert(peer_id);
    }

    /// Handle a peer disconnection.
    pub(crate) fn handle_disconnected(&mut self, peer_id: PeerId) {
        self.connected_peers.remove(&peer_id);
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

    /// Run the engine in its own task, processing commands and results.
    pub async fn run(mut self, mut commands_rx: mpsc::UnboundedReceiver<EngineCommand>) {
        loop {
            tokio::select! {
                // Handle commands from Behaviour
                Some(cmd) = commands_rx.recv() => {
                    self.handle_command(cmd).await;
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
                // TODO(AndrewL): Implement message broadcasting
                let _ = (channel, message, response);
                todo!()
            }
            EngineCommand::HandleHandlerOutput { peer_id, output } => {
                // TODO(AndrewL): Implement handler output processing
                let _ = (peer_id, output);
                todo!()
            }
            EngineCommand::HandleConnected { peer_id } => {
                self.handle_connected(peer_id);
            }
            EngineCommand::HandleDisconnected { peer_id } => {
                self.handle_disconnected(peer_id);
            }
        }
    }
}
