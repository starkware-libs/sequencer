use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use crate::server::metrics::{MetricsLayer, METRICS_PATH};
use crate::server::test_recorder::shared_handle;

fn fallthrough_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|_req: Request<HttpBody>| {
        let response = Response::builder()
            .status(StatusCode::IM_A_TEAPOT)
            .body(HttpBody::new(Full::new(Bytes::from_static(b"fallthrough"))))
            .expect("static body is infallible");
        futures::future::ready(Ok::<_, std::convert::Infallible>(response))
    })
}

fn empty_request(method: Method, path: &str) -> Request<HttpBody> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible")
}

async fn read_body(response: Response<HttpBody>) -> (StatusCode, Vec<u8>) {
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body collect").to_bytes().to_vec();
    (parts.status, bytes)
}

#[tokio::test]
async fn get_metrics_renders_prometheus_text() {
    // `shared_handle` installs the recorder exactly once across the test
    // binary; see `test_recorder.rs`.
    let handle = shared_handle().clone();
    let svc = MetricsLayer::new(handle).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, METRICS_PATH)).await.unwrap();

    let (status, body) = read_body(response).await;
    assert_eq!(status, StatusCode::OK);
    let body_text = String::from_utf8(body).unwrap();
    assert!(
        body_text.contains("prover_build_info"),
        "scrape should include build_info, got:\n{body_text}"
    );
    // Don't bind to specific label values — `shared_handle` uses generic
    // test labels and is also called by other tests. Verifying the
    // build_info series exists at all is sufficient.
    assert!(body_text.contains("version="));
    assert!(body_text.contains("git_sha="));
}
