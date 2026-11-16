use std::time::Duration;

use http::{Method, StatusCode};
use tower::{Layer, ServiceExt};

use super::{HEALTHY_BODY, SATURATED_BODY};
use crate::server::health::{HealthLayer, HEALTH_PATH};
use crate::server::middleware_test_utils::{
    empty_request,
    fallthrough_service,
    read_response,
    unsaturated_health_layer,
};
use crate::server::saturation::SaturationMonitor;

#[tokio::test]
async fn get_health_returns_200_with_json_body() {
    let svc = unsaturated_health_layer().layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();

    let (status, body, headers) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, HEALTHY_BODY);
    assert_eq!(headers.get(http::header::CONTENT_TYPE).unwrap(), "application/json");
}

#[tokio::test]
async fn non_get_health_falls_through() {
    let svc = unsaturated_health_layer().layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::POST, HEALTH_PATH)).await.unwrap();

    let (status, _body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}

#[tokio::test]
async fn get_other_path_falls_through() {
    let svc = unsaturated_health_layer().layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, "/")).await.unwrap();

    let (status, _body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}

#[tokio::test]
async fn saturated_for_at_least_threshold_returns_503_with_opaque_body() {
    let saturation_monitor = SaturationMonitor::default();
    saturation_monitor.mark_slot_acquired();
    saturation_monitor.mark_rejected();
    let svc = HealthLayer::new(saturation_monitor, Duration::ZERO).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();

    let (status, body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, SATURATED_BODY);
}

/// One of two proving runs finishes. The other still holds its slot, so only the release can
/// have cleared the window.
#[tokio::test]
async fn recovery_clears_saturation_and_health_returns_to_200() {
    let saturation_monitor = SaturationMonitor::default();
    saturation_monitor.mark_slot_acquired();
    saturation_monitor.mark_slot_acquired();
    saturation_monitor.mark_rejected();
    saturation_monitor.mark_slot_released();
    let svc = HealthLayer::new(saturation_monitor, Duration::ZERO).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();
    let (status, body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, HEALTHY_BODY);
}

/// A reject that lands after the last proving run finished leaves nothing to clear its window.
/// The probe must still report 200, or a drained pod would stay at 503 with no traffic to recover.
#[tokio::test]
async fn idle_service_returns_200_after_a_late_reject() {
    let saturation_monitor = SaturationMonitor::default();
    saturation_monitor.mark_slot_acquired();
    saturation_monitor.mark_slot_released();
    saturation_monitor.mark_rejected();
    let svc = HealthLayer::new(saturation_monitor, Duration::ZERO).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, HEALTH_PATH)).await.unwrap();
    let (status, body, _) = read_response(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, HEALTHY_BODY);
}
