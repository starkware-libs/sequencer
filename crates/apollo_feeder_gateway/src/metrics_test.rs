use metrics_exporter_prometheus::PrometheusBuilder;

use crate::metrics::{init_metrics, FEEDER_GATEWAY_REQUESTS_TOTAL};

#[test]
fn feeder_gateway_metrics_register_at_zero() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    init_metrics();

    let rendered = recorder.handle().render();
    FEEDER_GATEWAY_REQUESTS_TOTAL.assert_eq::<usize>(&rendered, 0);
}
