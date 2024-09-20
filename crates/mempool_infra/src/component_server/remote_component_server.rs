use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc::Sender;

use super::definitions::ComponentServerStarter;
use crate::component_client::{send_locally, ClientError};
use crate::component_definitions::{
    BincodeSerializable,
    ComponentRequestAndResponseSender,
    SerdeWrapper,
    ServerError,
    APPLICATION_OCTET_STREAM,
};

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
/// use async_trait::async_trait;
/// use serde::{Deserialize, Serialize};
/// use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
/// use tokio::task;
///
/// use crate::starknet_mempool_infra::component_definitions::ComponentRequestHandler;
/// use crate::starknet_mempool_infra::component_server::{
///     ComponentServerStarter,
///     RemoteComponentServer,
/// };
///
/// // Define your component
/// struct MyComponent {}
///
/// #[async_trait]
/// impl ComponentStarter for MyComponent {
///     async fn start(&mut self) -> Result<(), ComponentStartError> {
///         Ok(())
///     }
/// }
///
/// // Define your request and response types
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyRequest {
///     pub content: String,
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
///         MyResponse { content: request.content.clone() + " processed" }
///     }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // Instantiate a channel to communicate with component.
///     let (tx, _rx) = tokio::sync::mpsc::channel(32);
///
///     // Set the ip address and port of the server's socket.
///     let ip_address = std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
///     let port: u16 = 8080;
///
///     // Instantiate the server.
///     let mut server = RemoteComponentServer::<MyRequest, MyResponse>::new(tx, ip_address, port);
///
///     // Start the server in a new task.
///     task::spawn(async move {
///         server.start().await;
///     });
/// }
/// ```
pub struct RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + Sync + 'static,
    Response: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    socket: SocketAddr,
    tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Request, Response> RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + std::fmt::Debug + Send + Sync + 'static,
    Response: Serialize + DeserializeOwned + std::fmt::Debug + Send + Sync + 'static,
{
    pub fn new(
        tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
        ip_address: IpAddr,
        port: u16,
    ) -> Self {
        Self { tx, socket: SocketAddr::new(ip_address, port) }
    }

    async fn handler(
        http_request: HyperRequest<Body>,
        tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        let body_bytes = to_bytes(http_request.into_body()).await?;

        let http_response = match SerdeWrapper::<Request>::from_bincode(&body_bytes)
            .map_err(|e| ClientError::ResponseDeserializationFailure(Arc::new(e)))
            .map(|open| open.data)
        {
            Ok(request) => {
                let response = send_locally(tx, request).await;
                HyperResponse::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                    .body(Body::from(
                        SerdeWrapper { data: response }
                            .to_bincode()
                            .expect("Response serialization should succeed"),
                    ))
            }
            Err(error) => {
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    SerdeWrapper { data: server_error }
                        .to_bincode()
                        .expect("Server error serialization should succeed"),
                ))
            }
        }
        .expect("Response building should succeed");

        Ok(http_response)
    }
}

#[async_trait]
impl<Request, Response> ComponentServerStarter for RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Send + Sync + std::fmt::Debug + 'static,
    Response: Serialize + DeserializeOwned + Send + Sync + std::fmt::Debug + 'static,
{
    async fn start(&mut self) {
        let make_svc = make_service_fn(|_conn| {
            let tx = self.tx.clone();
            async { Ok::<_, hyper::Error>(service_fn(move |req| Self::handler(req, tx.clone()))) }
        });

        Server::bind(&self.socket.clone()).serve(make_svc).await.unwrap();
    }
}
