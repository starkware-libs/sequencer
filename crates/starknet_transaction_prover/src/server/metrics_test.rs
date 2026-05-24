use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};

use crate::server::metrics::{install_exporter, MetricsLayer, METRICS_PATH};

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
    // Note: install_exporter installs the global recorder, so this test must
    // be the only one in the crate that calls it. Other metric tests should
    // share this fixture or call install_exporter via `try_install`.
    let handle = install_exporter("0.0.1-test", "deadbeef").expect("install");
    let svc = MetricsLayer::new(handle).layer(fallthrough_service());

    let response = svc.oneshot(empty_request(Method::GET, METRICS_PATH)).await.unwrap();

    let (status, body) = read_body(response).await;
    assert_eq!(status, StatusCode::OK);
    let body_text = String::from_utf8(body).unwrap();
    assert!(
        body_text.contains("prover_build_info"),
        "scrape should include build_info, got:\n{body_text}"
    );
    assert!(body_text.contains("version=\"0.0.1-test\""));
    assert!(body_text.contains("git_sha=\"deadbeef\""));
}
