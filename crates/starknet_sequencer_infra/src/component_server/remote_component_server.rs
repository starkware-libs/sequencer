use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::component_client::{ClientError, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    RemoteServerConfig,
    ServerError,
    APPLICATION_OCTET_STREAM,
};
use crate::component_server::ComponentServerStarter;
use crate::errors::ComponentServerError;
use crate::serde_utils::BincodeSerdeWrapper;

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
/// use tokio::task;
///
/// use crate::starknet_sequencer_infra::component_client::LocalComponentClient;
/// use crate::starknet_sequencer_infra::component_definitions::{
///     ComponentRequestHandler,
///     ComponentStarter,
///     RemoteServerConfig,
/// };
/// use crate::starknet_sequencer_infra::component_server::{
///     ComponentServerStarter,
///     RemoteComponentServer,
/// };
/// use crate::starknet_sequencer_infra::errors::ComponentError;
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
///     // Instantiate a local client to communicate with component.
///     let (tx, _rx) = tokio::sync::mpsc::channel(32);
///     let local_client = LocalComponentClient::<MyRequest, MyResponse>::new(tx);
///
///     // Set the ip address and port of the server's socket.
///     let ip_address = std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
///     let port: u16 = 8080;
///     let config = RemoteServerConfig { socket: std::net::SocketAddr::new(ip_address, port) };
///
///     // Instantiate the server.
///     let mut server = RemoteComponentServer::<MyRequest, MyResponse>::new(local_client, config);
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
    local_client: LocalComponentClient<Request, Response>,
}

impl<Request, Response> RemoteComponentServer<Request, Response>
where
    Request: Serialize + DeserializeOwned + Debug + Send + Sync + 'static,
    Response: Serialize + DeserializeOwned + Debug + Send + Sync + 'static,
{
    pub fn new(
        local_client: LocalComponentClient<Request, Response>,
        config: RemoteServerConfig,
    ) -> Self {
        Self { local_client, socket: config.socket }
    }

    async fn remote_component_server_handler(
        http_request: HyperRequest<Body>,
        local_client: LocalComponentClient<Request, Response>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        let body_bytes = to_bytes(http_request.into_body()).await?;

        let http_response = match BincodeSerdeWrapper::<Request>::from_bincode(&body_bytes)
            .map_err(|e| ClientError::ResponseDeserializationFailure(Arc::new(e)))
        {
            Ok(request) => {
                let response = local_client.send(request).await;
                match response {
                    Ok(response) => HyperResponse::builder()
                        .status(StatusCode::OK)
                        .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                        .body(Body::from(
                            BincodeSerdeWrapper::new(response)
                                .to_bincode()
                                .expect("Response serialization should succeed"),
                        )),
                    Err(error) => {
                        panic!(
                            "Remote server failed sending with its local client. Error: {:?}",
                            error
                        );
                    }
                }
            }
            Err(error) => {
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    BincodeSerdeWrapper::new(server_error)
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
    Request: Serialize + DeserializeOwned + Send + Sync + Debug + 'static,
    Response: Serialize + DeserializeOwned + Send + Sync + Debug + 'static,
{
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        let make_svc = make_service_fn(|_conn| {
            let local_client = self.local_client.clone();
            async {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    Self::remote_component_server_handler(req, local_client.clone())
                }))
            }
        });

        Server::bind(&self.socket.clone())
            .serve(make_svc)
            .await
            .map_err(|err| ComponentServerError::HttpServerStartError(err.to_string()))?;
        Ok(())
    }
}
