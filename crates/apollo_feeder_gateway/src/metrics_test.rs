use std::sync::Arc;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use metrics_exporter_prometheus::PrometheusBuilder;
use tower::util::ServiceExt;

use crate::feeder_gateway::FeederGateway;
use crate::metrics::{init_metrics, FEEDER_GATEWAY_REQUESTS_TOTAL};
use crate::reader::MockChainDataReader;

#[test]
fn feeder_gateway_metrics_register_at_zero() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    init_metrics();

    let rendered = recorder.handle().render();
    FEEDER_GATEWAY_REQUESTS_TOTAL.assert_eq::<usize>(&rendered, 0);
}

#[tokio::test]
async fn request_metric_counts_api_requests_but_not_health_probes() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    init_metrics();

    let app =
        FeederGateway::new(FeederGatewayConfig::default(), Arc::new(MockChainDataReader::new()))
            .app();

    let api_request_uris =
        ["/feeder_gateway/get_public_key", "/feeder_gateway/get_contract_addresses"];
    let health_probe_uris = ["/feeder_gateway/is_alive", "/feeder_gateway/is_ready"];
    for uri in api_request_uris.iter().chain(&health_probe_uris) {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(*uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let rendered = recorder.handle().render();
    FEEDER_GATEWAY_REQUESTS_TOTAL.assert_eq::<usize>(&rendered, api_request_uris.len());
}
