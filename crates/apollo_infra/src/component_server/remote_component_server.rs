use std::collections::BTreeMap;
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::task::{Context, Poll};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::AddrIncoming;
use hyper::service::make_service_fn;
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tower::{service_fn, Service, ServiceExt};
use tracing::{debug, error, trace, warn, Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use validator::Validate;

use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::RemoteServerMetrics;
use crate::requests::LabeledRequest;
use crate::serde_utils::SerdeWrapper;
use crate::trace_util::extract_context_from_headers;

const DEFAULT_MAX_STREAMS_PER_CONNECTION: u32 = 8;
const DEFAULT_BIND_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

// The communication configuration of a local component server.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteServerConfig {
    pub max_streams_per_connection: u32,
    pub bind_ip: IpAddr,
    pub set_tcp_nodelay: bool,
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
        ])
    }
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            max_streams_per_connection: DEFAULT_MAX_STREAMS_PER_CONNECTION,
            bind_ip: DEFAULT_BIND_IP,
            set_tcp_nodelay: true,
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
    max_concurrency: usize,
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
        max_concurrency: usize,
        metrics: &'static RemoteServerMetrics,
    ) -> Self {
        metrics.register();
        Self { local_client, config: remote_server_config, port, max_concurrency, metrics }
    }

    async fn remote_component_server_handler(
        http_request: HyperRequest<Body>,
        local_client: LocalComponentClient<Request, Response>,
        metrics: &'static RemoteServerMetrics,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        // Extract trace context from incoming HTTP headers and set as parent of current span.
        let parent_context = extract_context_from_headers(http_request.headers());
        Span::current().set_parent(parent_context);

        trace!("Received HTTP request: {http_request:?}");
        let body_bytes = to_bytes(http_request.into_body()).await?;
        trace!("Extracted {} bytes from HTTP request body", body_bytes.len());

        metrics.increment_total_received();

        let http_response = match SerdeWrapper::<Request>::wrapper_deserialize(&body_bytes)
            .map_err(|err| ClientError::ResponseDeserializationFailure(err.to_string()))
        {
            Ok(request) => {
                trace!("Successfully deserialized request: {request:?}");
                metrics.increment_valid_received();

                // Wrap the send operation in a tokio::spawn as it is NOT a cancel-safe operation.
                // Even if the current task is cancelled, the inner task will continue to run.
                // Use .in_current_span() to propagate OpenTelemetry trace context to the spawned
                // task.
                let response =
                    tokio::spawn(async move { local_client.send(request).await }.in_current_span())
                        .await
                        .expect("Should be able to extract value from the task");

                metrics.increment_processed();

                match response {
                    Ok(response) => {
                        trace!("Local client processed request successfully: {response:?}");
                        HyperResponse::builder()
                            .status(StatusCode::OK)
                            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                            .body(Body::from(
                                SerdeWrapper::new(response)
                                    .wrapper_serialize()
                                    .expect("Response serialization should succeed"),
                            ))
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
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    SerdeWrapper::new(server_error)
                        .wrapper_serialize()
                        .expect("Server error serialization should succeed"),
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
    Request: Serialize + DeserializeOwned + Send + Debug + LabeledRequest + 'static,
    Response: Serialize + DeserializeOwned + Send + Debug + 'static,
{
    async fn start(&mut self) {
        let bind_socket = SocketAddr::new(self.config.bind_ip, self.port);
        debug!(
            "Starting server with socket {:?} with {:?} concurrent connections",
            bind_socket, self.max_concurrency
        );
        let connection_semaphore = Arc::new(Semaphore::new(self.max_concurrency));

        let make_svc = make_service_fn(|_conn| {
            let connection_semaphore = connection_semaphore.clone();
            let local_client = self.local_client.clone();
            let metrics = self.metrics;

            async move {
                match connection_semaphore.try_acquire_owned() {
                    Ok(permit) => {
                        metrics.increment_number_of_connections();
                        trace!("Acquired semaphore permit for connection");
                        let handle_request_service = service_fn(move |req| {
                            trace!("Received request: {:?}", req);
                            Self::remote_component_server_handler(
                                req,
                                local_client.clone(),
                                metrics,
                            )
                        })
                        .boxed();

                        // Bundle the service and the acquired permit to limit concurrency at the
                        // connection level.
                        let service = PermitGuardedService {
                            inner: handle_request_service,
                            _permit: Some(permit),
                            remote_server_metrics: metrics,
                        };
                        Ok::<_, hyper::Error>(service)
                    }
                    Err(_) => {
                        trace!("Too many connections, denying a new connection");
                        // Marked `async` to conform to the expected `Service` trait, requiring the
                        // handler to return a `Future`.
                        let reject_request_service = service_fn(move |_req| async {
                            let body: Vec<u8> =
                                SerdeWrapper::new(ServerError::RequestDeserializationFailure(
                                    BUSY_PREVIOUS_REQUESTS_MSG.to_string(),
                                ))
                                .wrapper_serialize()
                                .expect("Server error serialization should succeed");
                            let response: HyperResponse<Body> = HyperResponse::builder()
                                // Return a 503 Service Unavailable response to indicate that the server is
                                // busy, which should indicate the load balancer to divert the request to
                                // another server.
                                .status(StatusCode::SERVICE_UNAVAILABLE)
                                .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                                .body(Body::from(body))
                                .expect("Should be able to construct server http response.");
                            // Explicitly mention the type, helping the Rust compiler avoid
                            // Error type ambiguity.
                            let wrapped_response: Result<HyperResponse<Body>, hyper::Error> =
                                Ok(response);
                            wrapped_response
                        })
                        .boxed();

                        // No permit is acquired, so no need to hold one.
                        let service = PermitGuardedService {
                            inner: reject_request_service,
                            _permit: None,
                            remote_server_metrics: metrics,
                        };
                        Ok::<_, hyper::Error>(service)
                    }
                }
            }
        });

        let mut incoming = AddrIncoming::bind(&bind_socket).unwrap_or_else(|e| {
            panic!("Failed to bind remote component server socket {:#?}: {e}", bind_socket)
        });
        incoming.set_nodelay(self.config.set_tcp_nodelay);

        Server::builder(incoming)
            .http2_max_concurrent_streams(self.config.max_streams_per_connection)
            .serve(make_svc)
            .await
            .unwrap_or_else(|e| panic!("Remote component server start error: {e}"));
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
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
