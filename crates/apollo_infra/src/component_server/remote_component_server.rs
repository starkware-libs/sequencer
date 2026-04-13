use std::collections::BTreeMap;
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::validators::validate_positive;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::service::{service_fn, Service};
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder as ServerBuilder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use socket2::{SockRef, TcpKeepalive};
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{debug, error, instrument, trace, warn};
use validator::Validate;

use crate::component_client::remote_component_client::validate_keepalive_timeout_ms;
use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    RequestId,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
    REQUEST_ID_HEADER,
    TCP_KEEPALIVE_FACTOR,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::RemoteServerMetrics;
use crate::requests::LabeledRequest;
use crate::serde_utils::SerdeWrapper;

const DEFAULT_MAX_STREAMS_PER_CONNECTION: u32 = 8;
const DEFAULT_BIND_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
const DEFAULT_MAX_CONCURRENCY: usize = 128;
// 8 MiB — bounds memory materialized from a single request as defense in depth.
const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_KEEPALIVE_INTERVAL_MS: u64 = 30_000;
const DEFAULT_KEEPALIVE_TIMEOUT_MS: u64 = 10_000;
// Number of unanswered TCP keepalive probes before the OS declares the connection dead.
// 3 probes × keepalive_interval gives a ~90 s probe window at the default interval.
const TCP_KEEPALIVE_RETRIES: u32 = 3;

macro_rules! serve_connection {
    (
        $io:expr,
        $service:expr,
        $max_streams:expr,
        $keepalive_interval:expr,
        $keepalive_timeout:expr
    ) => {
        let result = ServerBuilder::new(TokioExecutor::new())
            .http2()
            .timer(TokioTimer::new())
            .max_concurrent_streams($max_streams)
            .keep_alive_interval($keepalive_interval)
            .keep_alive_timeout($keepalive_timeout)
            .serve_connection($io, $service)
            .await;

        if let Err(e) = result {
            error!("Remote component server start error: {e}");
        }
    };
}

// The communication configuration of a local component server.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteServerConfig {
    pub max_streams_per_connection: u32,
    pub bind_ip: IpAddr,
    pub set_tcp_nodelay: bool,
    #[validate(custom(function = "validate_positive"))]
    pub max_concurrency: usize,
    pub max_request_body_bytes: usize,
    pub keepalive_interval_ms: u64,
    #[validate(custom(function = "validate_keepalive_timeout_ms"))]
    pub keepalive_timeout_ms: u64,
}

impl SerializeConfig for RemoteServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "bind_ip",
                &self.bind_ip.to_string(),
                "Binding address of the remote component server.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_streams_per_connection",
                &self.max_streams_per_connection,
                "Maximal number of streams per HTTP connection.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "set_tcp_nodelay",
                &self.set_tcp_nodelay,
                "Whether to set TCP_NODELAY on the server responses.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_concurrency",
                &self.max_concurrency,
                "The maximum number of concurrent requests handling.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_request_body_bytes",
                &self.max_request_body_bytes,
                "Maximum allowed size in bytes for an incoming request body. Requests exceeding \
                 this limit are rejected with 413 Payload Too Large.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "keepalive_interval_ms",
                &self.keepalive_interval_ms,
                "Interval in milliseconds between HTTP/2 keepalive pings sent to the client.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "keepalive_timeout_ms",
                &self.keepalive_timeout_ms,
                "Timeout in milliseconds to wait for a keepalive ping response before closing the \
                 connection.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            max_streams_per_connection: DEFAULT_MAX_STREAMS_PER_CONNECTION,
            bind_ip: DEFAULT_BIND_IP,
            set_tcp_nodelay: true,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_request_body_bytes: DEFAULT_MAX_REQUEST_BODY_BYTES,
            keepalive_interval_ms: DEFAULT_KEEPALIVE_INTERVAL_MS,
            keepalive_timeout_ms: DEFAULT_KEEPALIVE_TIMEOUT_MS,
        }
    }
}

/// The `RemoteComponentServer` struct is a generic server that receives requests and returns
/// responses for a specified component, using HTTP connection.
pub struct RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    local_client: LocalComponentClient<Request, Response>,
    config: RemoteServerConfig,
    port: u16,
    metrics: &'static RemoteServerMetrics,
}

impl<Request, Response> RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Debug + Send + LabeledRequest + 'static,
    Response: Serialize + DeserializeOwned + Debug + Send + 'static,
{
    pub fn new(
        local_client: LocalComponentClient<Request, Response>,
        remote_server_config: RemoteServerConfig,
        port: u16,
        metrics: &'static RemoteServerMetrics,
    ) -> Self {
        metrics.register();
        Self { local_client, config: remote_server_config, port, metrics }
    }

    #[instrument(skip_all, fields(request_id = %request_id, remote_addr = %client_peer))]
    async fn remote_component_server_handler(
        http_request: HyperRequest<Incoming>,
        request_id: RequestId,
        client_peer: SocketAddr,
        local_client: LocalComponentClient<Request, Response>,
        metrics: &'static RemoteServerMetrics,
        max_request_body_bytes: usize,
    ) -> Result<HyperResponse<Full<Bytes>>, hyper::Error> {
        trace!("Received HTTP request: {http_request:?}");
        let body_bytes =
            match Limited::new(http_request.into_body(), max_request_body_bytes).collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(err) => {
                    warn!("Request body too large: {err}");
                    let server_error = ServerError::RequestBodyTooLarge(err.to_string());
                    return Ok(HyperResponse::builder()
                        .status(StatusCode::PAYLOAD_TOO_LARGE)
                        .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                        .body(Full::new(Bytes::from(
                            SerdeWrapper::new(server_error)
                                .wrapper_serialize()
                                .expect("Server error serialization should succeed"),
                        )))
                        .expect("Response building should succeed"));
                }
            };
        trace!("Extracted {} bytes from HTTP request body", body_bytes.len());

        metrics.increment_total_received();

        let http_response = match SerdeWrapper::<Request>::wrapper_deserialize(&body_bytes)
            .map_err(|err| ClientError::ResponseDeserializationFailure(err.to_string()))
        {
            Ok(request) => {
                trace!(
                    remote_addr = %client_peer,
                    request_id = %request_id,
                    request_type = request.request_label(),
                    "remote component request",
                );
                trace!("Successfully deserialized request: {request:?}");
                metrics.increment_valid_received();

                // Wrap the send operation in a tokio::spawn as it is NOT a cancel-safe operation.
                // Even if the current task is cancelled, the inner task will continue to run.
                // Note: this creates a new request ID for the local client.
                let response = tokio::spawn(async move { local_client.send(request).await })
                    .await
                    .expect("Should be able to extract value from the task");

                metrics.increment_processed();

                match response {
                    Ok(response) => {
                        trace!("Local client processed request successfully: {response:?}");
                        HyperResponse::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                            .body(Full::new(Bytes::from(
                                SerdeWrapper::new(response)
                                    .wrapper_serialize()
                                    .expect("Response serialization should succeed"),
                            )))
                    }
                    Err(error) => {
                        panic!(
                            "Remote server failed sending with its local client. Error: {error:?}"
                        );
                    }
                }
            }
            Err(error) => {
                error!("Failed to deserialize request: {error:?}");
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Full::new(
                    Bytes::from(
                        SerdeWrapper::new(server_error)
                            .wrapper_serialize()
                            .expect("Server error serialization should succeed"),
                    ),
                ))
            }
        }
        .expect("Response building should succeed");
        trace!("Built HTTP response: {http_response:?}");

        Ok(http_response)
    }
}

#[async_trait]
impl<Request, Response> ComponentServerStarter for RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + Sync + Debug + LabeledRequest + 'static,
    Response: Serialize + DeserializeOwned + Send + Sync + Debug + 'static,
{
    async fn start(&mut self) {
        let bind_socket = SocketAddr::new(self.config.bind_ip, self.port);
        debug!(
            "Starting server with socket {:?} with {:?} concurrent connections",
            bind_socket, self.config.max_concurrency
        );
        let connection_semaphore = Arc::new(Semaphore::new(self.config.max_concurrency));

        let per_connection_service =
            |io: TokioIo<tokio::net::TcpStream>,
             max_streams: u32,
             keepalive_interval: Duration,
             keepalive_timeout: Duration,
             connection_semaphore: Arc<Semaphore>,
             local_client: LocalComponentClient<Request, Response>,
             metrics: &'static RemoteServerMetrics,
             max_request_body_bytes: usize,
             client_peer: SocketAddr| {
                async move {
                    trace!(remote_addr = %client_peer, "remote component TCP connection opened");
                    match connection_semaphore.try_acquire_owned() {
                        Ok(permit) => {
                            metrics.increment_number_of_connections();
                            trace!("Acquired semaphore permit for connection");
                            let client_peer_for_handler = client_peer;
                            let handle_request_service =
                                service_fn(move |req: HyperRequest<Incoming>| {
                                    trace!("Received request: {:?}", req);
                                    let request_id = req
                                        .headers()
                                        .get(REQUEST_ID_HEADER)
                                        .and_then(|header| header.to_str().ok())
                                        .and_then(|s| s.parse::<RequestId>().ok())
                                        .expect(
                                            "Request ID should be present in the request headers",
                                        );
                                    Self::remote_component_server_handler(
                                        req,
                                        request_id,
                                        client_peer_for_handler,
                                        local_client.clone(),
                                        metrics,
                                        max_request_body_bytes,
                                    )
                                });

                            // Bundle the service and the acquired permit to limit concurrency at
                            // the connection level.
                            let service = PermitGuardedService {
                                inner: handle_request_service,
                                _permit: Some(permit),
                                remote_server_metrics: metrics,
                            };

                            serve_connection!(
                                io,
                                service,
                                max_streams,
                                keepalive_interval,
                                keepalive_timeout
                            );
                            trace!(remote_addr = %client_peer, "remote component TCP connection closed");
                        }
                        Err(_) => {
                            trace!("Too many connections, denying a new connection");
                            // Marked `async` to conform to the expected `Service` trait, requiring
                            // the handler to return a `Future`.
                            let reject_request_service =
                                service_fn(move |_req: HyperRequest<Incoming>| async {
                                    let body: Vec<u8> = SerdeWrapper::new(
                                        ServerError::RequestDeserializationFailure(
                                            BUSY_PREVIOUS_REQUESTS_MSG.to_string(),
                                        ),
                                    )
                                    .wrapper_serialize()
                                    .expect("Server error serialization should succeed");
                                    let response: HyperResponse<Full<Bytes>> =
                                        HyperResponse::builder()
                                    // Return a 503 Service Unavailable response to indicate that
                                    // the server is busy, which should indicate the load balancer
                                    // to divert the request to another server.
                                    .status(StatusCode::SERVICE_UNAVAILABLE)
                                    .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                                    .body(Full::new(Bytes::from(body)))
                                    .expect("Should be able to construct server http response.");
                                    // Explicitly mention the type, helping the Rust compiler avoid
                                    // Error type ambiguity.
                                    let wrapped_response: Result<
                                        HyperResponse<Full<Bytes>>,
                                        hyper::Error,
                                    > = Ok(response);
                                    wrapped_response
                                });

                            // No permit is acquired, so no need to hold one.
                            let service = PermitGuardedService {
                                inner: reject_request_service,
                                _permit: None,
                                remote_server_metrics: metrics,
                            };

                            serve_connection!(
                                io,
                                service,
                                max_streams,
                                keepalive_interval,
                                keepalive_timeout
                            );
                            trace!(remote_addr = %client_peer, "remote component TCP connection closed");
                        }
                    }
                }
            };

        let listener = TcpListener::bind(&bind_socket).await.unwrap_or_else(|e| {
            panic!("Failed to bind remote component server socket {:#?}: {e}", bind_socket)
        });

        let max_streams = self.config.max_streams_per_connection;
        let keepalive_interval = Duration::from_millis(self.config.keepalive_interval_ms);
        let keepalive_timeout = Duration::from_millis(self.config.keepalive_timeout_ms);

        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to accept connection: {e}");
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
            };

            if let Err(e) = stream.set_nodelay(self.config.set_tcp_nodelay) {
                warn!("Failed to set TCP_NODELAY: {e}");
            }

            let tcp_keepalive = TcpKeepalive::new()
                .with_time(keepalive_timeout.mul_f64(TCP_KEEPALIVE_FACTOR))
                .with_interval(keepalive_interval)
                .with_retries(TCP_KEEPALIVE_RETRIES);
            if let Err(e) = SockRef::from(&stream).set_tcp_keepalive(&tcp_keepalive) {
                error!("Failed to set TCP keepalive: {e}");
            }

            let io = TokioIo::new(stream);

            tokio::spawn(per_connection_service(
                io,
                max_streams,
                keepalive_interval,
                keepalive_timeout,
                connection_semaphore.clone(),
                self.local_client.clone(),
                self.metrics,
                self.config.max_request_body_bytes,
                peer_addr,
            ));
        }
    }
}

impl<Request, Response> Drop for RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    fn drop(&mut self) {
        warn!("Dropping {}.", short_type_name::<Self>());
    }
}

// TODO(Tsabary): consider moving this to `apollo_infra_utils`, and applying it on the http server
// as well.
/// A service wrapper that holds an `OwnedSemaphorePermit` for the lifetime of a connection.
/// This ensures that the permit is only released when the service (and thus the connection) is
/// dropped, achieving connection-level concurrency limits in asynchronous servers. Transparently
/// delegates all service calls to the inner service.
struct PermitGuardedService<S> {
    inner: S,
    _permit: Option<OwnedSemaphorePermit>,
    remote_server_metrics: &'static RemoteServerMetrics,
}

impl<S, Req> Service<Req> for PermitGuardedService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, req: Req) -> Self::Future {
        self.inner.call(req)
    }
}

impl<S> Drop for PermitGuardedService<S> {
    fn drop(&mut self) {
        if self._permit.is_some() {
            self.remote_server_metrics.decrement_number_of_connections();
        }
    }
}
