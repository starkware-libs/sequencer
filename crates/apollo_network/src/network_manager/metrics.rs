use std::collections::HashMap;

use apollo_metrics::metrics::{MetricCounter, MetricGauge};
use libp2p::gossipsub::TopicHash;

pub struct BroadcastNetworkMetrics {
    pub num_sent_broadcast_messages: MetricCounter,
    pub num_received_broadcast_messages: MetricCounter,
}

impl BroadcastNetworkMetrics {
    pub fn register(&self) {
        self.num_sent_broadcast_messages.register();
        self.num_received_broadcast_messages.register();
    }
}

pub struct SqmrNetworkMetrics {
    pub num_active_inbound_sessions: MetricGauge,
    pub num_active_outbound_sessions: MetricGauge,
}

impl SqmrNetworkMetrics {
    pub fn register(&self) {
        self.num_active_inbound_sessions.register();
        self.num_active_inbound_sessions.set(0f64);
        self.num_active_outbound_sessions.register();
        self.num_active_outbound_sessions.set(0f64);
    }
}

pub struct GossipsubMetrics {
    /// Number of peers in the mesh network (peers we directly exchange messages with)
    pub num_mesh_peers: MetricGauge,
    /// Total number of known peers and their subscribed topics  
    pub num_all_peers: MetricGauge,
    /// Number of topics we are currently subscribed to
    pub num_subscribed_topics: MetricGauge,
    /// Number of peers with gossipsub protocol support
    pub num_gossipsub_peers: MetricGauge,
    /// Number of peers with floodsub protocol support  
    pub num_floodsub_peers: MetricGauge,
    /// Average number of subscribed topics per peer
    pub avg_topics_per_peer: MetricGauge,
    /// Maximum number of subscribed topics by any single peer
    pub max_topics_per_peer: MetricGauge,
    /// Minimum number of subscribed topics by any single peer (for peers with >0 topics)
    pub min_topics_per_peer: MetricGauge,
    /// Total number of topic subscriptions across all peers
    pub total_topic_subscriptions: MetricGauge,
    /// Average mesh peers per topic that we're subscribed to
    pub avg_mesh_peers_per_topic: MetricGauge,
    /// Maximum mesh peers for any single topic we're subscribed to
    pub max_mesh_peers_per_topic: MetricGauge,
    /// Minimum mesh peers for any single topic we're subscribed to
    pub min_mesh_peers_per_topic: MetricGauge,
    /// Number of peers with positive peer scores (if peer scoring is enabled)
    pub num_peers_with_positive_score: MetricGauge,
    /// Number of peers with negative peer scores (if peer scoring is enabled)
    pub num_peers_with_negative_score: MetricGauge,
    /// Average peer score across all scored peers (if peer scoring is enabled)
    pub avg_peer_score: MetricGauge,

    // event metrics
    pub count_event_messages_received: MetricCounter,
    pub count_event_peer_subscribed: MetricCounter,
    pub count_event_peer_unsubscribed: MetricCounter,
    pub count_event_gossipsub_not_supported: MetricCounter,
    pub count_event_slow_peers: MetricCounter,
}

impl GossipsubMetrics {
    pub fn register(&self) {
        self.num_mesh_peers.register();
        self.num_all_peers.register();
        self.num_subscribed_topics.register();
        self.num_gossipsub_peers.register();
        self.num_floodsub_peers.register();
        self.avg_topics_per_peer.register();
        self.max_topics_per_peer.register();
        self.min_topics_per_peer.register();
        self.total_topic_subscriptions.register();
        self.avg_mesh_peers_per_topic.register();
        self.max_mesh_peers_per_topic.register();
        self.min_mesh_peers_per_topic.register();
        self.num_peers_with_positive_score.register();
        self.num_peers_with_negative_score.register();
        self.avg_peer_score.register();
        self.count_event_messages_received.register();
        self.count_event_peer_subscribed.register();
        self.count_event_peer_unsubscribed.register();
        self.count_event_gossipsub_not_supported.register();
        self.count_event_slow_peers.register();
    }
}

// TODO(alonl, shahak): Consider making these fields private and receive Topics instead of
// TopicHashes in the constructor
pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_blacklisted_peers: MetricGauge,
    pub broadcast_metrics_by_topic: Option<HashMap<TopicHash, BroadcastNetworkMetrics>>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
    pub gossipsub_metrics: Option<GossipsubMetrics>,
}

impl NetworkMetrics {
    pub fn register(&self) {
        self.num_connected_peers.register();
        self.num_connected_peers.set(0f64);
        self.num_blacklisted_peers.register();
        self.num_blacklisted_peers.set(0f64);
        if let Some(broadcast_metrics_by_topic) = self.broadcast_metrics_by_topic.as_ref() {
            for broadcast_metrics in broadcast_metrics_by_topic.values() {
                broadcast_metrics.register();
            }
        }
        if let Some(sqmr_metrics) = self.sqmr_metrics.as_ref() {
            sqmr_metrics.register();
        }
        if let Some(gossipsub_metrics) = self.gossipsub_metrics.as_ref() {
            gossipsub_metrics.register();
        }
    }
}
