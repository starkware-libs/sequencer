use std::net::IpAddr;

use axum::body::Body;
use axum::http::Request;

use crate::monitoring_endpoint::MONITORING_PREFIX;

// TODO(Tsabary): Clean feature dependencies and dev dependencies.

pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
