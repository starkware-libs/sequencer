use std::fmt::Debug;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::limit::ConcurrencyLimitLayer;
use tower::ServiceBuilder;
use tracing::{debug, error, trace, warn};

use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{ComponentClient, ServerError, APPLICATION_OCTET_STREAM};
use crate::component_server::ComponentServerStarter;
use crate::metrics::RemoteServerMetrics;
use crate::serde_utils::SerdeWrapper;

/// The `RemoteComponentServer` struct is a generic server that handles requests and responses for a
/// specified component. It receives requests, processes them using the provided component, and
/// sends back responses. The server needs to be started using the `start` function, which runs
/// indefinitely.
///
/// # Type Parameters
///
/// - `Component`: The type of the component that will handle the requests. This type must implement
///   the `ComponentRequestHandler` trait, which defines how the component processes requests and
///   generates responses.
/// - `Request`: The type of requests that the component will handle. This type must implement the
///   `serde::de::DeserializeOwned` (e.g. by using #[derive(Deserialize)]) trait.
/// - `Response`: The type of responses that the component will generate. This type must implement
///   the `Serialize` trait.
///
/// # Fields
///
/// - `component`: The component responsible for handling the requests and generating responses.
/// - `socket`: A socket address for the server to listen on.
///
/// # Example
/// ```rust
/// // Example usage of the RemoteComponentServer
/// use apollo_metrics::metrics::{MetricCounter, MetricScope};
/// use async_trait::async_trait;
/// use serde::{Deserialize, Serialize};
/// use tokio::task;
///
/// use crate::apollo_infra::component_client::LocalComponentClient;
/// use crate::apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
/// use crate::apollo_infra::component_server::{ComponentServerStarter, RemoteComponentServer};
/// use crate::apollo_infra::metrics::RemoteServerMetrics;
///
/// const REMOTE_MESSAGES_RECEIVED: MetricCounter = MetricCounter::new(
///     MetricScope::Infra,
///     "remote_received_messages_counter",
///     "remote_received_messages_counter_filter",
///     "Received remote messages counter",
///     0,
/// );
/// const REMOTE_VALID_MESSAGES_RECEIVED: MetricCounter = MetricCounter::new(
///     MetricScope::Infra,
///     "remote_valid_received_messages_counter",
///     "remote_valid_received_messages_counter_filter",
///     "Received remote valid messages counter",
///     0,
/// );
/// const REMOTE_MESSAGES_PROCESSED: MetricCounter = MetricCounter::new(
///     MetricScope::Infra,
///     "remote_processed_messages_counter",
///     "remote_processed_messages_counter_filter",
///     "Processed messages counter",
///     0,
/// );
///
/// // Define your component
/// struct MyComponent {}
///
/// impl ComponentStarter for MyComponent {}
///
/// // Define your request and response types
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// impl AsRef<str> for MyRequest {
///     fn as_ref(&self) -> &str {
///         &self.content
///     }
/// }
///
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyResponse {
///     content: String,
/// }
///
/// // Define your request processing logic
/// #[async_trait]
/// impl ComponentRequestHandler<MyRequest, MyResponse> for MyComponent {
///     async fn handle_request(&mut self, request: MyRequest) -> MyResponse {
///         MyResponse { content: request.content + " processed" }
///     }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // Instantiate a local client to communicate with component.
///     let (tx, _rx) = tokio::sync::mpsc::channel(32);
///     let local_client = LocalComponentClient::<MyRequest, MyResponse>::new(tx);
///
///     // Set the ip address and port of the server's socket.
///     let ip_address = std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
///     let port: u16 = 8080;
///     let max_concurrency = 10;
///
///     // Instantiate the server.
///     let mut server = RemoteComponentServer::<MyRequest, MyResponse>::new(
///         local_client,
///         ip_address,
///         port,
///         max_concurrency,
///         RemoteServerMetrics::new(
///             &REMOTE_MESSAGES_RECEIVED,
///             &REMOTE_VALID_MESSAGES_RECEIVED,
///             &REMOTE_MESSAGES_PROCESSED,
///         ),
///     );
///
///     // Start the server in a new task.
///     task::spawn(async move {
///         server.start().await;
///     });
/// }
/// ```
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
        trace!("Received HTTP request: {:?}", http_request);
        let body_bytes = to_bytes(http_request.into_body()).await?;
        trace!("Extracted {} bytes from HTTP request body", body_bytes.len());

        metrics.increment_total_received();

        let http_response = match SerdeWrapper::<Request>::wrapper_deserialize(&body_bytes)
            .map_err(|err| ClientError::ResponseDeserializationFailure(err.to_string()))
        {
            Ok(request) => {
                trace!("Successfully deserialized request: {:?}", request);
                metrics.increment_valid_received();

                // Wrap the send operation in a tokio::spawn as it is NOT a cancel-safe operation.
                // Even if the current task is cancelled, the inner task will continue to run.
                let response = tokio::spawn(async move { local_client.send(request).await })
                    .await
                    .expect("Should be able to extract value from the task");

                metrics.increment_processed();

                match response {
                    Ok(response) => {
                        trace!("Local client processed request successfully: {:?}", response);
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
                            "Remote server failed sending with its local client. Error: {:?}",
                            error
                        );
                    }
                }
            }
            Err(error) => {
                error!("Failed to deserialize request: {:?}", error);
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    SerdeWrapper::new(server_error)
                        .wrapper_serialize()
                        .expect("Server error serialization should succeed"),
                ))
            }
        }
        .expect("Response building should succeed");
        trace!("Built HTTP response: {:?}", http_response);

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
        debug!("Starting server on socket: {:?}", self.socket);
        let make_svc = make_service_fn(|_conn| {
            let local_client = self.local_client.clone();
            let max_concurrency = self.max_concurrency;
            debug!(
                "Initializing service for new connection with max_concurrency: {:?}",
                max_concurrency
            );
            let metrics = self.metrics.clone();
            async move {
                let app_service = service_fn(move |req| {
                    trace!("Received request: {:?}", req);
                    Self::remote_component_server_handler(
                        req,
                        local_client.clone(),
                        metrics.clone(),
                    )
                });

                // Apply the ConcurrencyLimitLayer middleware
                let service = ServiceBuilder::new()
                    .layer(ConcurrencyLimitLayer::new(max_concurrency))
                    .service(app_service);

                Ok::<_, hyper::Error>(service)
            }
        });
        debug!("Binding server to socket: {:?}", self.socket);
        Server::bind(&self.socket)
            .serve(make_svc)
            .await
            .unwrap_or_else(|e| panic!("HttpServerStartError: {}", e));
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
