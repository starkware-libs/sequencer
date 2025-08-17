use std::fmt::Debug;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::task::{Context, Poll};

use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::make_service_fn;
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tower::{service_fn, Service, ServiceExt};
use tracing::{debug, error, trace, warn};

use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ServerError,
    APPLICATION_OCTET_STREAM,
    BUSY_PREVIOUS_REQUESTS_MSG,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::RemoteServerMetrics;
use crate::serde_utils::SerdeWrapper;

/// The `RemoteComponentServer` struct is a generic server that receives requests and returns
/// responses for a specified component, using HTTP connection.
// TODO(alonl): change the metrics to a static reference.
pub struct RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + 'static,
    Response: Serialize + DeserializeOwned + Send + 'static,
{
    socket: SocketAddr,
    local_client: LocalComponentClient<Request, Response>,
    max_concurrency: usize,
    metrics: Arc<RemoteServerMetrics>,
}

impl<Request, Response> RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Debug + Send + 'static,
    Response: Serialize + DeserializeOwned + Debug + Send + 'static,
{
    pub fn new(
        local_client: LocalComponentClient<Request, Response>,
        ip: IpAddr,
        port: u16,
        max_concurrency: usize,
        metrics: RemoteServerMetrics,
    ) -> Self {
        metrics.register();
        Self {
            local_client,
            socket: SocketAddr::new(ip, port),
            max_concurrency,
            metrics: Arc::new(metrics),
        }
    }

    async fn remote_component_server_handler(
        http_request: HyperRequest<Body>,
        local_client: LocalComponentClient<Request, Response>,
        metrics: Arc<RemoteServerMetrics>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
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
    Request: Serialize + DeserializeOwned + Send + Debug + 'static,
    Response: Serialize + DeserializeOwned + Send + Debug + 'static,
{
    async fn start(&mut self) {
        debug!(
            "Starting server with socket {:?} with {:?} concurrent connections",
            self.socket, self.max_concurrency
        );
        let connection_semaphore = Arc::new(Semaphore::new(self.max_concurrency));

        let make_svc = make_service_fn(|_conn| {
            let connection_semaphore = connection_semaphore.clone();
            let local_client = self.local_client.clone();
            let metrics = self.metrics.clone();

            async move {
                match connection_semaphore.try_acquire_owned() {
                    Ok(permit) => {
                        metrics.increment_number_of_connections();
                        let remote_server_metrics = metrics.clone();
                        trace!("Acquired semaphore permit for connection");
                        let handle_request_service = service_fn(move |req| {
                            trace!("Received request: {:?}", req);
                            Self::remote_component_server_handler(
                                req,
                                local_client.clone(),
                                remote_server_metrics.clone(),
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

        Server::bind(&self.socket)
            .tcp_nodelay(true)        // enable TCP_NODELAY for accepted connections
            .serve(make_svc)
            .await
            .unwrap_or_else(|e| panic!("Remote component server start error: {}", e));
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
    remote_server_metrics: Arc<RemoteServerMetrics>,
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
