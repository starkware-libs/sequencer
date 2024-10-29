use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body::combinators::UnsyncBoxBody;
use pretty_assertions::assert_eq;
use tower::ServiceExt;

use super::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
    MonitoringEndpointConfig,
    MONITORING_PREFIX,
};

const TEST_VERSION: &str = "1.2.3-dev";

fn setup_monitoring_endpont() -> MonitoringEndpoint {
    create_monitoring_endpoint(MonitoringEndpointConfig::default(), TEST_VERSION)
}

async fn request_app(
    app: Router,
    method: &str,
) -> Response<UnsyncBoxBody<axum::body::Bytes, axum::Error>> {
    app.oneshot(
        Request::builder()
            .uri(format!("/{MONITORING_PREFIX}/{method}").as_str())
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn version() {
    let response = request_app(setup_monitoring_endpont().app(), "nodeVersion").await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    assert_eq!(&body[..], TEST_VERSION.as_bytes());
}

#[tokio::test]
async fn alive() {
    let response = request_app(setup_monitoring_endpont().app(), "alive").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready() {
    let response = request_app(setup_monitoring_endpont().app(), "ready").await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn run_server() {
    tokio::spawn(async move { setup_monitoring_endpont().run().await });
    tokio::task::yield_now().await;

    let MonitoringEndpointConfig { ip, port } = MonitoringEndpointConfig::default();

    let client = hyper::Client::new();

    let response = client
        .request(
            Request::builder()
                .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/nodeVersion"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
