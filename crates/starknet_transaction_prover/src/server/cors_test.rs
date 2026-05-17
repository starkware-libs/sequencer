use std::convert::Infallible;

use assert_matches::assert_matches;
use http::{header, HeaderValue, Method, Request, Response};
use rstest::rstest;
use tower::{Layer, ServiceExt};
use tower_http::cors::CorsLayer;

use crate::errors::ConfigError;
use crate::server::cors::{build_cors_layer, cors_mode, normalize_cors_allow_origins};

#[test]
fn test_build_cors_layer_disabled_when_no_origins() {
    assert!(build_cors_layer(&[]).unwrap().is_none());
}

/// Trivial inner service returning an empty 200, used to exercise the `CorsLayer` built by
/// `build_cors_layer` with real requests/responses.
async fn ok_service(_request: Request<()>) -> Result<Response<()>, Infallible> {
    Ok(Response::new(()))
}

/// Builds the `CorsLayer` for the given origins, panicking if CORS would be disabled.
fn cors_layer_for(origins: &[&str]) -> CorsLayer {
    let origins: Vec<String> = origins.iter().map(|origin| origin.to_string()).collect();
    build_cors_layer(&origins).unwrap().expect("test origins are non-empty")
}

fn request_with_origin(method: Method, origin: &str) -> Request<()> {
    Request::builder().method(method).uri("/").header(header::ORIGIN, origin).body(()).unwrap()
}

#[tokio::test]
async fn test_cors_layer_allowlist_echoes_allowed_origin() {
    let svc = cors_layer_for(&["http://example.com"]).layer(tower::service_fn(ok_service));

    let response =
        svc.oneshot(request_with_origin(Method::GET, "http://example.com")).await.unwrap();

    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "http://example.com"
    );
}

#[tokio::test]
async fn test_cors_layer_allowlist_omits_header_for_disallowed_origin() {
    let svc = cors_layer_for(&["http://example.com"]).layer(tower::service_fn(ok_service));

    let response = svc.oneshot(request_with_origin(Method::GET, "http://evil.com")).await.unwrap();

    assert!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn test_cors_layer_wildcard_allows_any_origin() {
    let svc = cors_layer_for(&["*"]).layer(tower::service_fn(ok_service));

    let response =
        svc.oneshot(request_with_origin(Method::GET, "http://example.com")).await.unwrap();

    assert_eq!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
}

#[tokio::test]
async fn test_cors_layer_preflight_allows_post_method() {
    let svc = cors_layer_for(&["http://example.com"]).layer(tower::service_fn(ok_service));
    let mut preflight_request = request_with_origin(Method::OPTIONS, "http://example.com");
    preflight_request
        .headers_mut()
        .insert(header::ACCESS_CONTROL_REQUEST_METHOD, HeaderValue::from_static("POST"));

    let response = svc.oneshot(preflight_request).await.unwrap();

    let allow_methods =
        response.headers().get(header::ACCESS_CONTROL_ALLOW_METHODS).unwrap().to_str().unwrap();
    assert!(allow_methods.contains("POST"));
}

#[rstest]
#[case::disabled(&[], "disabled")]
#[case::wildcard(&["*"], "wildcard")]
#[case::allowlist(&["http://example.com"], "allowlist")]
#[case::multiple_origins(&["http://a.com", "http://b.com"], "allowlist")]
fn test_cors_mode_labels(#[case] origins: &[&str], #[case] expected_label: &str) {
    let origins: Vec<String> = origins.iter().map(|origin| origin.to_string()).collect();
    assert_eq!(cors_mode(&origins), expected_label);
}

#[rstest]
#[case::ftp_scheme(&["ftp://example.com"])]
#[case::empty_host(&["http://"])]
#[case::userinfo(&["http://user:pass@example.com"])]
#[case::path(&["http://example.com/path"])]
#[case::query(&["http://example.com?q=1"])]
#[case::fragment(&["http://example.com#frag"])]
#[case::unparseable(&["not a url"])]
fn test_normalize_rejects_invalid_origin(#[case] origins: &[&str]) {
    let origins: Vec<String> = origins.iter().map(|origin| origin.to_string()).collect();
    assert_matches!(normalize_cors_allow_origins(origins), Err(ConfigError::InvalidArgument(_)));
}

#[rstest]
#[case::strip_http_default_port(&["http://example.com:80"], &["http://example.com"])]
#[case::strip_https_default_port(&["https://example.com:443"], &["https://example.com"])]
#[case::preserve_non_default_port(&["http://example.com:8080"], &["http://example.com:8080"])]
#[case::dedup_equivalent_origins(
    &["http://example.com", "http://example.com:80"],
    &["http://example.com"],
)]
#[case::wildcard_collapses_others(
    &["http://example.com", "*", "https://foo.bar"],
    &["*"],
)]
#[case::multiple_distinct_origins_retained_in_order(
    &["http://a.com", "http://b.com"],
    &["http://a.com", "http://b.com"],
)]
fn test_normalize_valid_origin(#[case] input: &[&str], #[case] expected: &[&str]) {
    let input: Vec<String> = input.iter().map(|origin| origin.to_string()).collect();
    let expected: Vec<String> = expected.iter().map(|origin| origin.to_string()).collect();
    assert_eq!(normalize_cors_allow_origins(input).unwrap(), expected);
}
