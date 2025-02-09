use starknet_sequencer_metrics::metrics::{MetricCounter, MetricGauge};

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

pub struct SqmrNetworkMetrics {}

impl SqmrNetworkMetrics {
    pub fn register(&self) {}
}

pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_active_inbound_sessions: MetricGauge,
    pub num_active_outbound_sessions: MetricGauge,
    pub broadcast_metrics: Option<BroadcastNetworkMetrics>,
    pub sqmr_metrics: Option<SqmrNetworkMetrics>,
}

impl NetworkMetrics {
    pub fn register(&self) {
        let num_connected_peers_metric = self.num_connected_peers.register();
        num_connected_peers_metric.set(0f64);
        let num_active_inbound_sessions_metric = self.num_active_inbound_sessions.register();
        num_active_inbound_sessions_metric.set(0f64);
        let num_active_outbound_sessions_metric = self.num_active_outbound_sessions.register();
        num_active_outbound_sessions_metric.set(0f64);
        if let Some(broadcast_metrics) = self.broadcast_metrics.as_ref() {
            broadcast_metrics.register();
        }
        if let Some(sqmr_metrics) = self.sqmr_metrics.as_ref() {
            sqmr_metrics.register();
        }
    }
}
