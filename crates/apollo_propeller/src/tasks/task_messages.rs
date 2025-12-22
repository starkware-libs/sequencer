//! Message types for inter-task communication.

use libp2p::identity::PeerId;

use crate::types::{Channel, Event, MessageRoot};
use crate::unit::PropellerUnit;

/// Messages sent from Validator task to State Manager task.
#[derive(Debug)]
pub(crate) enum ValidatorToStateManager {
    /// A unit has been successfully validated.
    ValidatedUnit {
        /// The peer that sent the unit.
        sender: PeerId,
        /// The validated unit.
        unit: PropellerUnit,
    },
    /// The validator task has stopped due to timeout.
    ValidatorStopped,
}

/// Messages sent from State Manager task to Core task.
#[derive(Debug)]
pub(crate) enum StateManagerToCore {
    /// An event to be emitted by the behaviour.
    Event(Event),
    /// The message processing has been finalized.
    Finalized {
        /// The channel of the finalized message.
        channel: Channel,
        /// The publisher of the finalized message.
        publisher: PeerId,
        /// The merkle root of the finalized message.
        message_root: MessageRoot,
    },
    /// Broadcast a unit to the specified peers (gossip).
    BroadcastUnit {
        /// The unit to broadcast.
        unit: crate::unit::PropellerUnit,
        /// The peers to broadcast to.
        peers: Vec<PeerId>,
    },
}

/// Messages sent from State Manager task to Validator task.
#[derive(Debug)]
pub(crate) enum StateManagerToValidator {
    /// Shutdown the validator task gracefully.
    Shutdown,
}
