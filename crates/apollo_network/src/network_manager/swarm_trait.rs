use futures::stream::Stream;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use tracing::{info, warn};

use super::BroadcastedMessageMetadata;
use crate::gossipsub_impl::Topic;
use crate::misconduct_score::MisconductScore;
use crate::peer_manager::ReputationModifier;
use crate::sqmr::behaviour::SessionIdNotFoundError;
use crate::sqmr::{InboundSessionId, OutboundSessionId, SessionId};
use crate::{mixed_behaviour, Bytes};
pub type Event = SwarmEvent<<mixed_behaviour::MixedBehaviour as NetworkBehaviour>::ToSwarm>;

pub trait SwarmTrait: Stream<Item = Event> + Unpin {
    fn send_response(
        &mut self,
        response: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn send_query(&mut self, query: Vec<u8>, protocol: StreamProtocol) -> OutboundSessionId;

    fn dial(&mut self, peer_multiaddr: Multiaddr) -> Result<(), DialError>;

    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError>;

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour;

    fn get_peer_id_from_session_id(
        &self,
        session_id: SessionId,
    ) -> Result<PeerId, SessionIdNotFoundError>;

    fn add_external_address(&mut self, address: Multiaddr);

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError>;

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash);

    fn report_peer_as_malicious(&mut self, peer_id: PeerId, misconduct_score: MisconductScore);

    fn add_new_supported_inbound_protocol(&mut self, protocol_name: StreamProtocol);

    fn continue_propagation(&mut self, message_metadata: BroadcastedMessageMetadata);
}

impl SwarmTrait for Swarm<mixed_behaviour::MixedBehaviour> {
    fn send_response(
        &mut self,
        response: Vec<u8>,
        inbound_session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().sqmr.send_response(response, inbound_session_id)
    }

    fn send_query(&mut self, query: Vec<u8>, protocol: StreamProtocol) -> OutboundSessionId {
        self.behaviour_mut().sqmr.start_query(query, protocol)
    }

    fn dial(&mut self, peer_multiaddr: Multiaddr) -> Result<(), DialError> {
        self.dial(DialOpts::from(peer_multiaddr))
    }

    fn close_inbound_session(
        &mut self,
        session_id: InboundSessionId,
    ) -> Result<(), SessionIdNotFoundError> {
        self.behaviour_mut().sqmr.close_inbound_session(session_id)
    }

    fn behaviour_mut(&mut self) -> &mut mixed_behaviour::MixedBehaviour {
        self.behaviour_mut()
    }

    fn get_peer_id_from_session_id(
        &self,
        session_id: SessionId,
    ) -> Result<PeerId, SessionIdNotFoundError> {
        self.behaviour()
            .sqmr
            .get_peer_id_and_connection_id_from_session_id(session_id)
            .map(|(peer_id, _)| peer_id)
    }

    fn add_external_address(&mut self, address: Multiaddr) {
        info!("Found new external address of this node: {address:?}");
        self.add_external_address(address);
    }

    fn subscribe_to_topic(&mut self, topic: &Topic) -> Result<(), SubscriptionError> {
        self.behaviour_mut().gossipsub.subscribe(topic).map(|_| ())
    }

    fn broadcast_message(&mut self, message: Bytes, topic_hash: TopicHash) {
        let result = self.behaviour_mut().gossipsub.publish(topic_hash.clone(), message);
        if let Err(err) = result {
            // TODO(shahak): Consider reporting to the subscriber broadcast failures or retrying
            // upon failure.
            warn!(
                "Error occured while broadcasting a message to the topic with hash \
                 {topic_hash:?}: {err:?}"
            );
        }
    }

    fn report_peer_as_malicious(&mut self, peer_id: PeerId, misconduct_score: MisconductScore) {
        let _ = self
            .behaviour_mut()
            .peer_manager
            .report_peer(peer_id, ReputationModifier::Misconduct { misconduct_score });
    }

    fn add_new_supported_inbound_protocol(&mut self, protocol: StreamProtocol) {
        self.behaviour_mut().sqmr.add_new_supported_inbound_protocol(protocol);
    }

    // TODO(shahak): Implement this function.
    fn continue_propagation(&mut self, _message_metadata: BroadcastedMessageMetadata) {}
}
