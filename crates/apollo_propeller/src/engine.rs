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
}
