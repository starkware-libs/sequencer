use futures::stream::Stream;
use libp2p::gossipsub::{SubscriptionError, TopicHash};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use tracing::{info, warn};

use super::BroadcastedMessageMetadata;
use crate::gossipsub_impl::Topic;
use crate::misconduct_score::MisconductScore;
use crate::network_manager::metrics::NetworkMetrics;
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

    fn update_metrics(&self, metrics: &NetworkMetrics);
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

    fn update_metrics(&self, metrics: &NetworkMetrics) {
        let Some(gossipsub_metrics) = &metrics.gossipsub_metrics else { return };
        let gossipsub = &self.behaviour().gossipsub;

        // Helper to convert usize counts to f64 metrics
        let set_count = |gauge: &apollo_metrics::metrics::MetricGauge, count: usize| {
            gauge.set(f64::from(u32::try_from(count).unwrap_or(u32::MAX)));
        };

        // Basic counts
        set_count(&gossipsub_metrics.num_mesh_peers, gossipsub.all_mesh_peers().count());
        set_count(&gossipsub_metrics.num_subscribed_topics, gossipsub.topics().count());

        // Collect peer data once for analysis
        let all_peers: Vec<_> = gossipsub.all_peers().collect();
        set_count(&gossipsub_metrics.num_all_peers, all_peers.len());
        set_count(&gossipsub_metrics.num_gossipsub_peers, gossipsub.peer_protocol().count());
        gossipsub_metrics.num_floodsub_peers.set(0.0); // Currently all peers are gossipsub

        // Topic subscription analysis
        let topic_counts: Vec<usize> = all_peers.iter().map(|(_, topics)| topics.len()).collect();
        let total_subscriptions: usize = topic_counts.iter().sum();
        set_count(&gossipsub_metrics.total_topic_subscriptions, total_subscriptions);

        if topic_counts.is_empty() {
            [
                &gossipsub_metrics.avg_topics_per_peer,
                &gossipsub_metrics.max_topics_per_peer,
                &gossipsub_metrics.min_topics_per_peer,
            ]
            .iter()
            .for_each(|metric| metric.set(0.0));
        } else {
            let avg = f64::from(u32::try_from(total_subscriptions).unwrap_or(u32::MAX)) / f64::from(u32::try_from(topic_counts.len()).unwrap_or(u32::MAX));
            gossipsub_metrics.avg_topics_per_peer.set(avg);

            if let (Some(&max), Some(&min_non_zero)) =
                (topic_counts.iter().max(), topic_counts.iter().filter(|&&c| c > 0).min())
            {
                set_count(&gossipsub_metrics.max_topics_per_peer, max);
                set_count(&gossipsub_metrics.min_topics_per_peer, min_non_zero);
            }
        }

        // Mesh analysis per topic
        let our_topics: Vec<_> = gossipsub.topics().collect();
        if our_topics.is_empty() {
            [
                &gossipsub_metrics.avg_mesh_peers_per_topic,
                &gossipsub_metrics.max_mesh_peers_per_topic,
                &gossipsub_metrics.min_mesh_peers_per_topic,
            ]
            .iter()
            .for_each(|metric| metric.set(0.0));
        } else {
            let mesh_counts: Vec<usize> =
                our_topics.iter().map(|topic| gossipsub.mesh_peers(topic).count()).collect();
            let total_mesh = mesh_counts.iter().sum::<usize>();
            let avg_mesh = f64::from(u32::try_from(total_mesh).unwrap_or(u32::MAX)) / f64::from(u32::try_from(our_topics.len()).unwrap_or(u32::MAX));
            gossipsub_metrics.avg_mesh_peers_per_topic.set(avg_mesh);

            if let (Some(&min), Some(&max)) = (mesh_counts.iter().min(), mesh_counts.iter().max()) {
                set_count(&gossipsub_metrics.min_mesh_peers_per_topic, min);
                set_count(&gossipsub_metrics.max_mesh_peers_per_topic, max);
            }
        }

        // Peer scoring analysis
        let peer_scores: Vec<f64> =
            all_peers.iter().filter_map(|(peer_id, _)| gossipsub.peer_score(peer_id)).collect();
        if peer_scores.is_empty() {
            [
                &gossipsub_metrics.num_peers_with_positive_score,
                &gossipsub_metrics.num_peers_with_negative_score,
                &gossipsub_metrics.avg_peer_score,
            ]
            .iter()
            .for_each(|metric| metric.set(0.0));
        } else {
            let positive_count = peer_scores.iter().filter(|&&score| score > 0.0).count();
            let negative_count = peer_scores.iter().filter(|&&score| score < 0.0).count();
            let avg_score = peer_scores.iter().sum::<f64>() / f64::from(u32::try_from(peer_scores.len()).unwrap_or(u32::MAX));

            set_count(&gossipsub_metrics.num_peers_with_positive_score, positive_count);
            set_count(&gossipsub_metrics.num_peers_with_negative_score, negative_count);
            gossipsub_metrics.avg_peer_score.set(avg_score);
        }
    }
}
