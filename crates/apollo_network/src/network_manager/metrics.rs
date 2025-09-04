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

// TODO(alonl, shahak): Consider making these fields private and receive Topics instead of
// TopicHashes in the constructor
pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_blacklisted_peers: MetricGauge,
    pub broadcast_metrics_by_topic: Option<HashMap<TopicHash, BroadcastNetworkMetrics>>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
    /// Set the desired prefix for the libp2p metrics.
    /// If `None`, the metrics will not be registered.
    /// If `Some(prefix)`, the metrics will be registered with the prefix.
    pub libp2p_metrics_prefix: Option<String>,
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
        // if let Some(libp2p_metrics) = self.libp2p_metrics.as_ref() {
        //     libp2p_metrics.register();
        // }
    }
}

// pub struct LibP2PMetrics {
//     pub num_connected_peers: MetricGauge,
//     pub num_blacklisted_peers: MetricGauge,
// }

// impl LibP2PMetrics {
//     pub fn register(&self) {
//         self.num_connected_peers.register();
//         self.num_blacklisted_peers.register();
//     }
// }

// /// Must implement debug for conversion
// impl Debug for LibP2PMetrics {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("LibP2PMetrics")
//             .field("num_connected_peers", &self.num_connected_peers.get_scope())
//             .field("num_blacklisted_peers", &self.num_blacklisted_peers.get_scope())
//             .finish()
//     }
// }
