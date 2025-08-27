use std::collections::HashMap;

use apollo_metrics::metrics::{MetricCounter, MetricGauge};
use libp2p::gossipsub::TopicHash;

pub struct BroadcastNetworkMetrics {
    pub num_sent_broadcast_messages: MetricCounter,
    pub num_dropped_broadcast_messages: MetricCounter,
    pub num_received_broadcast_messages: MetricCounter,
}

impl BroadcastNetworkMetrics {
    pub fn register(&self) {
        self.num_sent_broadcast_messages.register();
        self.num_dropped_broadcast_messages.register();
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

pub struct EventMetrics {
    // Swarm events
    pub connections_established: MetricCounter,
    pub connections_closed: MetricCounter,
    pub dial_failure: MetricCounter,
    pub listen_failure: MetricCounter,
    pub listen_error: MetricCounter,
    pub address_change: MetricCounter,
    pub new_listeners: MetricCounter,
    pub new_listen_addrs: MetricCounter,
    pub expired_listen_addrs: MetricCounter,
    pub listener_closed: MetricCounter,
    pub new_external_addr_candidate: MetricCounter,
    pub external_addr_confirmed: MetricCounter,
    pub external_addr_expired: MetricCounter,
    pub new_external_addr_of_peer: MetricCounter,

    // Connection handler events
    pub inbound_connections_handled: MetricCounter,
    pub outbound_connections_handled: MetricCounter,
    pub connection_handler_events: MetricCounter,
}

impl EventMetrics {
    pub fn register(&self) {
        self.connections_established.register();
        self.connections_closed.register();
        self.dial_failure.register();
        self.listen_failure.register();
        self.listen_error.register();
        self.address_change.register();
        self.new_listeners.register();
        self.new_listen_addrs.register();
        self.expired_listen_addrs.register();
        self.listener_closed.register();
        self.new_external_addr_candidate.register();
        self.external_addr_confirmed.register();
        self.external_addr_expired.register();
        self.new_external_addr_of_peer.register();
        self.inbound_connections_handled.register();
        self.outbound_connections_handled.register();
        self.connection_handler_events.register();
    }
}

// TODO(alonl, shahak): Consider making these fields private and receive Topics instead of
// TopicHashes in the constructor
pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_blacklisted_peers: MetricGauge,
    pub broadcast_metrics_by_topic: Option<HashMap<TopicHash, BroadcastNetworkMetrics>>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
    pub event_metrics: Option<EventMetrics>,
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
        if let Some(event_metrics) = self.event_metrics.as_ref() {
            event_metrics.register();
        }
    }
}
