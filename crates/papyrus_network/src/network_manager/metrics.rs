use starknet_sequencer_metrics::metrics::MetricGauge;
pub struct NetworkMetrics {
    pub num_connected_peers: MetricGauge,
    pub num_active_inbound_sessions: MetricGauge,
    pub num_active_outbound_sessions: MetricGauge,
}

impl NetworkMetrics {
    pub fn register(&self) {
        let num_connected_peers_metric = self.num_connected_peers.register();
        num_connected_peers_metric.set(0f64);
        let num_active_inbound_sessions_metric = self.num_active_inbound_sessions.register();
        num_active_inbound_sessions_metric.set(0f64);
        let num_active_outbound_sessions_metric = self.num_active_outbound_sessions.register();
        num_active_outbound_sessions_metric.set(0f64);
    }
}
