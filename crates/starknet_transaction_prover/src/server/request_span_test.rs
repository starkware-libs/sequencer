use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use jsonrpsee::server::HttpBody;
use tower::{Layer, ServiceExt};
use tower_ohttp::Decapsulated;

use crate::server::request_log::{RequestLogLayer, REQUEST_ID_HEADER};
use crate::server::request_span::RequestSpanLayer;

/// Inner service that echoes the request's `x-request-id` into the response
/// body, so tests can observe which id `RequestSpanLayer` bound.
fn echo_id_service() -> impl tower::Service<
    Request<HttpBody>,
    Response = Response<HttpBody>,
    Error = std::convert::Infallible,
    Future = futures::future::Ready<Result<Response<HttpBody>, std::convert::Infallible>>,
> + Clone {
    tower::service_fn(|request: Request<HttpBody>| {
        let id = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default();
        futures::future::ready(Ok::<_, std::convert::Infallible>(
            Response::builder()
                .status(StatusCode::OK)
                .body(HttpBody::new(Full::new(Bytes::from(id))))
                .expect("static body is infallible"),
        ))
    })
}

async fn body_string(response: Response<HttpBody>) -> String {
    let bytes = response.into_body().collect().await.expect("body collect").to_bytes().to_vec();
    String::from_utf8(bytes).expect("utf8 body")
}

#[tokio::test]
async fn plaintext_reuses_inbound_request_id() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .header(REQUEST_ID_HEADER, "reused-xyz")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let response = RequestSpanLayer.layer(echo_id_service()).oneshot(request).await.unwrap();

    assert_eq!(body_string(response).await, "reused-xyz");
}

#[tokio::test]
async fn decapsulated_gets_fresh_id_discarding_inbound() {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .header(REQUEST_ID_HEADER, "envelope-abc")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");
    request.extensions_mut().insert(Decapsulated);

    let response = RequestSpanLayer.layer(echo_id_service()).oneshot(request).await.unwrap();

    let id = body_string(response).await;
    assert_ne!(id, "envelope-abc", "must discard the client-supplied inner id");
    assert!(uuid::Uuid::parse_str(&id).is_ok(), "must mint a fresh UUID, got {id:?}");
}

/// The cross-layer plaintext contract: with `RequestLogLayer` (outer) stacked
/// over `RequestSpanLayer` (inner) and no inbound id, the id the outer layer
/// generates and echoes on the response must be the same id the inner layer
/// binds for the handler — one shared id end-to-end.
#[tokio::test]
async fn plaintext_log_and_span_layers_share_generated_id() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/")
        .body(HttpBody::new(Full::new(Bytes::new())))
        .expect("static body is infallible");

    let svc = RequestLogLayer.layer(RequestSpanLayer.layer(echo_id_service()));
    let response = svc.oneshot(request).await.unwrap();

    let echoed_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .expect("response carries the id")
        .to_str()
        .unwrap()
        .to_owned();
    let handler_id = body_string(response).await;

    assert_eq!(echoed_id, handler_id, "echoed response id must equal the id the handler saw");
    assert!(
        uuid::Uuid::parse_str(&handler_id).is_ok(),
        "generated id must be a UUID, got {handler_id:?}"
    );
}
