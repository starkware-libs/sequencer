//! Propeller core logic.
//!
//! This module contains the protocol logic (broadcasting, validation, reconstruction, channel
//! management).

use libp2p::identity::PeerId;
use tokio::sync::mpsc;

use crate::config::Config;

/// Commands sent from Behaviour to Core.
#[derive(Debug)]
pub(crate) enum CoreCommand {
    // TODO(AndrewL): Add command variants as needed
}

/// Outputs emitted by the core (polled by Behaviour).
#[derive(Debug)]
pub(crate) enum CoreOutput {
    // TODO(AndrewL): Add output variants as needed
}

/// Core protocol logic.
pub(crate) struct Core {
    _local_peer_id: PeerId,
    _config: Config,
}

impl Core {
    /// Create a new Core instance.
    pub fn new(local_peer_id: PeerId, config: Config) -> Self {
        Self { _local_peer_id: local_peer_id, _config: config }
    }

    /// Run the core task loop.
    pub async fn run(
        self,
        mut commands_rx: mpsc::Receiver<CoreCommand>,
        _outputs_tx: mpsc::Sender<CoreOutput>,
    ) {
        while let Some(command) = commands_rx.recv().await {
            self.handle_command(command);
        }
    }

    fn handle_command(&self, _command: CoreCommand) {
        todo!("Handle core commands")
    }
}
