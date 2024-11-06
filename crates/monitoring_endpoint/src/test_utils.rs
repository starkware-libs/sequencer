use std::net::IpAddr;

use axum::body::Body;
use axum::http::Request;

use crate::monitoring_endpoint::MONITORING_PREFIX;

// TODO(Tsabary): Clean feature dependencies and dev dependencies.

// TODO(Tsabary): To be used in the next pr, remove the annotation.
#[allow(dead_code)]
pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
