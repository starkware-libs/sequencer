use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

use super::definitions::ComponentServerStarter;
use crate::component_definitions::{
    ComponentRequestHandler,
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
///   `serde::de::DeserializeOwned` trait.
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
/// #[derive(Deserialize)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// #[derive(Serialize)]
/// struct MyResponse {
///     pub content: String,
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
///     // Instantiate the component.
///     let component = MyComponent {};
///
///     // Set the ip address and port of the server's socket.
///     let ip_address = std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
///     let port: u16 = 8080;
///
///     // Instantiate the server.
///     let mut server = RemoteComponentServer::<MyComponent, MyRequest, MyResponse>::new(
///         component, ip_address, port,
///     );
///
///     // Start the server in a new task.
///     task::spawn(async move {
///         server.start().await;
///     });
/// }
/// ```
pub struct RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + 'static,
    Response: Serialize + 'static,
{
    socket: SocketAddr,
    component: Arc<Mutex<Component>>,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Component, Request, Response> RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + 'static,
    Response: Serialize + 'static,
{
    pub fn new(component: Component, ip_address: IpAddr, port: u16) -> Self {
        Self {
            component: Arc::new(Mutex::new(component)),
            socket: SocketAddr::new(ip_address, port),
            _req: PhantomData,
            _res: PhantomData,
        }
    }

    async fn handler(
        http_request: HyperRequest<Body>,
        component: Arc<Mutex<Component>>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        let body_bytes = to_bytes(http_request.into_body()).await?;
        let http_response = match deserialize(&body_bytes) {
            Ok(component_request) => {
                // Acquire the lock for component computation, release afterwards.
                let component_response =
                    { component.lock().await.handle_request(component_request).await };
                HyperResponse::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                    .body(Body::from(
                        serialize(&component_response)
                            .expect("Response serialization should succeed"),
                    ))
            }
            Err(error) => {
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    serialize(&server_error).expect("Server error serialization should succeed"),
                ))
            }
        }
        .expect("Response building should succeed");

        Ok(http_response)
    }
}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + Sync + 'static,
    Response: Serialize + Send + Sync + 'static,
{
    async fn start(&mut self) {
        let make_svc = make_service_fn(|_conn| {
            let component = Arc::clone(&self.component);
            async {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    Self::handler(req, Arc::clone(&component))
                }))
            }
        });

        Server::bind(&self.socket.clone()).serve(make_svc).await.unwrap();
    }
}
