use std::sync::Arc;

use apollo_feeder_gateway_config::config::FeederGatewayConfig;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

use crate::feeder_gateway::FeederGateway;
use crate::reader::MockChainDataReader;

#[tokio::test]
async fn is_alive_returns_ok() {
    let reader = Arc::new(MockChainDataReader::new());
    let feeder_gateway = FeederGateway::new(FeederGatewayConfig::default(), reader);
    let app = feeder_gateway.app();

    let response = app
        .oneshot(Request::builder().uri("/feeder_gateway/is_alive").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
