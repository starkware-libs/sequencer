//! Message processor combining validation and state management.
//!
//! This module merges the validator and state manager tasks into a single task
//! to eliminate shared fate coordination complexity while maintaining parallelism
//! between validation and reconstruction operations.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use libp2p::identity::{PeerId, PublicKey};
use tokio::sync::mpsc;
use tokio::time::sleep_until;

use crate::tree::PropellerScheduleManager;
use crate::types::{Channel, Event, MessageRoot};
use crate::unit::PropellerUnit;
use crate::ShardIndex;

pub type UnitToValidate = (PeerId, PropellerUnit);

/// Messages sent from MessageProcessor to Engine.
#[derive(Debug)]
pub enum StateManagerToEngine {
    /// An event to be emitted by the behaviour.
    Event(Event),
    /// The message processing has been finalized.
    Finalized { channel: Channel, publisher: PeerId, message_root: MessageRoot },
    /// Broadcast a unit to the specified peers
    BroadcastUnit { unit: PropellerUnit, peers: Vec<PeerId> },
}

/// Message processor that handles validation and state management for a single message.
pub struct MessageProcessor {
    // Message identification (needed across methods)
    pub channel: Channel,
    pub publisher: PeerId,
    pub message_root: MessageRoot,
    pub my_shard_index: ShardIndex,

    // Components (needed across methods)
    pub publisher_public_key: PublicKey,
    pub tree_manager: Arc<PropellerScheduleManager>,
    pub local_peer_id: PeerId,

    // Communication channels (needed across methods)
    pub unit_rx: mpsc::UnboundedReceiver<UnitToValidate>,
    pub engine_tx: mpsc::UnboundedSender<StateManagerToEngine>,

    // Timeout
    pub timeout: Duration,
}

impl MessageProcessor {
    pub async fn run(mut self) {
        tracing::trace!(
            "[MSG_PROC] Started for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );

        // Local state variables
        let deadline = tokio::time::Instant::now() + self.timeout;

        // TODO(AndrewL): remove this
        #[allow(clippy::never_loop)]
        loop {
            tokio::select! {
                _ = sleep_until(deadline) => {
                    let _ = self.emit_timeout_and_finalize().await;
                    break;
                }
            }
        }

        tracing::trace!(
            "[MSG_PROC] Stopped for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );
    }

    async fn emit_timeout_and_finalize(&mut self) -> ControlFlow<()> {
        tracing::trace!(
            "[MSG_PROC] Timeout reached for channel={:?} publisher={:?} root={:?}",
            self.channel,
            self.publisher,
            self.message_root
        );

        self.emit_and_finalize(Event::MessageTimeout {
            channel: self.channel,
            publisher: self.publisher,
            message_root: self.message_root,
        })
    }

    fn emit_and_finalize(&self, event: Event) -> ControlFlow<()> {
        self.engine_tx.send(StateManagerToEngine::Event(event)).expect("Engine task has exited");
        self.engine_tx
            .send(StateManagerToEngine::Finalized {
                channel: self.channel,
                publisher: self.publisher,
                message_root: self.message_root,
            })
            .expect("Engine task has exited");
        ControlFlow::Break(())
    }
}
