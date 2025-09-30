use apollo_propeller as propeller;

use crate::mixed_behaviour::BridgedBehaviour;
use crate::{mixed_behaviour, Bytes};

#[derive(Debug)]
pub enum ExternalEvent {
    /// A complete message has been reconstructed from shreds.
    MessageReceived {
        /// The publisher of the message.
        publisher: libp2p::PeerId,
        /// The message id the message belongs to.
        message_id: propeller::MessageId,
        /// The reconstructed message data.
        data: Bytes,
    },
}

impl From<propeller::Event> for mixed_behaviour::Event {
    fn from(event: propeller::Event) -> Self {
        match event {
            propeller::Event::MessageReceived { message_id, message: data, publisher } => {
                mixed_behaviour::Event::ExternalEvent(mixed_behaviour::ExternalEvent::Propeller(
                    ExternalEvent::MessageReceived { message_id, data, publisher },
                ))
            }
            e => {
                tracing::error!("Unexpected propeller event: {e:?}");
                mixed_behaviour::Event::ToOtherBehaviourEvent(
                    mixed_behaviour::ToOtherBehaviourEvent::NoOp,
                )
            }
        }
    }
}

impl BridgedBehaviour for propeller::Behaviour {
    fn on_other_behaviour_event(&mut self, _event: &mixed_behaviour::ToOtherBehaviourEvent) {}
}
