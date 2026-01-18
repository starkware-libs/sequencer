use std::collections::BTreeMap;
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
// TODO(victork): finalise migration to hyper 1.x
use bytes::Bytes;
use http_1::header::CONTENT_TYPE;
use http_1::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper_1::body::Incoming;
use hyper_1::service::{service_fn, Service};
use hyper_1::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ServerBuilder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{debug, error, instrument, trace, warn};
use validator::Validate;

use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    RequestId,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
    REQUEST_ID_HEADER,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::RemoteServerMetrics;
use crate::requests::LabeledRequest;
use crate::serde_utils::SerdeWrapper;

const DEFAULT_MAX_STREAMS_PER_CONNECTION: u32 = 8;
const DEFAULT_BIND_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

macro_rules! serve_connection {
    ($io:expr, $service:expr, $max_streams:expr) => {
        let result = ServerBuilder::new(TokioExecutor::new())
            .http2()
            .max_concurrent_streams($max_streams)
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

    #[instrument(skip_all,fields(request_id = %request_id))]
    async fn remote_component_server_handler(
        http_request: HyperRequest<Incoming>,
        request_id: RequestId,
        local_client: LocalComponentClient<Request, Response>,
        metrics: &'static RemoteServerMetrics,
    ) -> Result<HyperResponse<Full<Bytes>>, hyper_1::Error> {
        trace!("Received HTTP request: {http_request:?}");
        let body_bytes = http_request.into_body().collect().await?.to_bytes();
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
            bind_socket, self.max_concurrency
        );
        let connection_semaphore = Arc::new(Semaphore::new(self.max_concurrency));

        let per_connection_service =
            |io: TokioIo<tokio::net::TcpStream>,
             max_streams: u32,
             connection_semaphore: Arc<Semaphore>,
             local_client: LocalComponentClient<Request, Response>,
             metrics: &'static RemoteServerMetrics| {
                async move {
                    match connection_semaphore.try_acquire_owned() {
                        Ok(permit) => {
                            metrics.increment_number_of_connections();
                            trace!("Acquired semaphore permit for connection");
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
                                        local_client.clone(),
                                        metrics,
                                    )
                                });

                            // Bundle the service and the acquired permit to limit concurrency at
                            // the connection level.
                            let service = PermitGuardedService {
                                inner: handle_request_service,
                                _permit: Some(permit),
                                remote_server_metrics: metrics,
                            };

                            serve_connection!(io, service, max_streams);
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
                                        hyper_1::Error,
                                    > = Ok(response);
                                    wrapped_response
                                });

                            // No permit is acquired, so no need to hold one.
                            let service = PermitGuardedService {
                                inner: reject_request_service,
                                _permit: None,
                                remote_server_metrics: metrics,
                            };

                            serve_connection!(io, service, max_streams);
                        }
                    }
                }
            };

        let listener = TcpListener::bind(&bind_socket).await.unwrap_or_else(|e| {
            panic!("Failed to bind remote component server socket {:#?}: {e}", bind_socket)
        });

        loop {
            let (stream, _) = match listener.accept().await {
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

            let io = TokioIo::new(stream);
            let max_streams = self.config.max_streams_per_connection;

            tokio::spawn(per_connection_service(
                io,
                max_streams,
                connection_semaphore.clone(),
                self.local_client.clone(),
                self.metrics,
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
