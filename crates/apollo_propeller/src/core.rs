//! Propeller core logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, channel
//! management).

use std::collections::HashMap;

use libp2p::identity::{PeerId, PublicKey};
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::tree::PropellerTreeManager;
use crate::types::{Channel, PeerSetError};

/// Commands sent from Behaviour to Core.
#[derive(Debug)]
pub(crate) enum CoreCommand {
    RegisterChannelPeers {
        channel: Channel,
        peers: Vec<(PeerId, u64, Option<PublicKey>)>,
        response: oneshot::Sender<Result<(), PeerSetError>>,
    },
}

/// Outputs emitted by the core (polled by Behaviour).
#[derive(Debug)]
pub(crate) enum CoreOutput {
    // TODO(AndrewL): Add output variants as needed
}

/// Data associated with a single channel.
struct ChannelData {
    _tree_manager: PropellerTreeManager,
}

/// Core protocol logic.
pub(crate) struct Core {
    local_peer_id: PeerId,
    channels: HashMap<Channel, ChannelData>,
}

impl Core {
    /// Create a new Core instance.
    pub fn new(local_peer_id: PeerId, _config: Config) -> Self {
        Self { local_peer_id, channels: HashMap::new() }
    }

    /// Run the core task loop.
    pub async fn run(
        mut self,
        mut commands_rx: mpsc::Receiver<CoreCommand>,
        _outputs_tx: mpsc::Sender<CoreOutput>,
    ) {
        while let Some(command) = commands_rx.recv().await {
            self.handle_command(command).await;
        }
    }

    async fn handle_command(&mut self, command: CoreCommand) {
        match command {
            CoreCommand::RegisterChannelPeers { channel, peers, response } => {
                let peer_weights: Vec<(PeerId, u64)> =
                    peers.iter().map(|(id, weight, _)| (*id, *weight)).collect();
                let mut tree_manager = PropellerTreeManager::new(self.local_peer_id);
                let result = tree_manager.update_nodes(peer_weights).map(|_| {
                    self.channels.insert(channel, ChannelData { _tree_manager: tree_manager });
                });
                let _ = response.send(result);
            }
        }
    }
}
