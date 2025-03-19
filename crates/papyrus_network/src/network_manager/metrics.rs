use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

// TODO(alonl): consider splitting the metrics by topic
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

pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_blacklisted_peers: MetricGauge,
    pub broadcast_metrics: Option<BroadcastNetworkMetrics>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
}

impl NetworkMetrics {
    pub fn register(&self) {
        self.num_connected_peers.register();
        self.num_connected_peers.set(0f64);
        self.num_blacklisted_peers.register();
        self.num_blacklisted_peers.set(0f64);
        if let Some(broadcast_metrics) = self.broadcast_metrics.as_ref() {
            broadcast_metrics.register();
        }
        if let Some(sqmr_metrics) = self.sqmr_metrics.as_ref() {
            sqmr_metrics.register();
        }
    }
}
