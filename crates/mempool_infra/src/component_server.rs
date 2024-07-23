use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::component_definitions::{
    ComponentRequestAndResponseSender, ComponentRequestHandler, ServerError,
    APPLICATION_OCTET_STREAM,
};
use crate::component_runner::ComponentStarter;

/// The `LocalComponentServer` struct is a generic server that handles requests and responses for a
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
///   `Send` and `Sync` traits to ensure safe concurrency.
/// - `Response`: The type of responses that the component will generate. This type must implement
///   the `Send` and `Sync` traits to ensure safe concurrency.
///
/// # Fields
///
/// - `component`: The component responsible for handling the requests and generating responses.
/// - `rx`: A receiver that receives incoming requests along with a sender to send back the
///   responses. This receiver is of type ` Receiver<ComponentRequestAndResponseSender<Request,
///   Response>>`.
///
/// # Example
/// ```rust
/// // Example usage of the LocalComponentServer
/// use std::sync::mpsc::{channel, Receiver};
///
/// use async_trait::async_trait;
/// use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
/// use tokio::task;
///
/// use crate::starknet_mempool_infra::component_definitions::{
///     ComponentRequestAndResponseSender, ComponentRequestHandler,
/// };
/// use crate::starknet_mempool_infra::component_server::{
///     ComponentServerStarter, LocalComponentServer,
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
/// struct MyRequest {
///     pub content: String,
/// }
///
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
///     // Create a channel for sending requests and receiving responses
///     let (tx, rx) = tokio::sync::mpsc::channel::<
///         ComponentRequestAndResponseSender<MyRequest, MyResponse>,
///     >(100);
///
///     // Instantiate the component.
///     let component = MyComponent {};
///
///     // Instantiate the server.
///     let mut server = LocalComponentServer::new(component, rx);
///
///     // Start the server in a new task.
///     task::spawn(async move {
///         server.start().await;
///     });
///
///     // Ensure the server starts running.
///     task::yield_now().await;
///
///     // Create the request and the response channel.
///     let (res_tx, mut res_rx) = tokio::sync::mpsc::channel::<MyResponse>(1);
///     let request = MyRequest { content: "request example".to_string() };
///     let request_and_res_tx = ComponentRequestAndResponseSender { request, tx: res_tx };
///
///     // Send the request.
///     tx.send(request_and_res_tx).await.unwrap();
///
///     // Receive the response.
///     let response = res_rx.recv().await.unwrap();
///     assert!(response.content == "request example processed".to_string(), "Unexpected response");
/// }
/// ```
pub struct LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter,
    Request: Send + Sync,
    Response: Send + Sync,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Component, Request, Response> LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter,
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    ) -> Self {
        Self { component, rx }
    }
}

#[async_trait]
pub trait ComponentServerStarter: Send + Sync {
    async fn start(&mut self);
}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    async fn start(&mut self) {
        if start_component(&mut self.component).await {
            while let Some(request_and_res_tx) = self.rx.recv().await {
                let request = request_and_res_tx.request;
                let tx = request_and_res_tx.tx;

                let res = self.component.handle_request(request).await;

                tx.send(res).await.expect("Response connection should be open.");
            }
        }
    }
}

pub async fn start_component<Component>(component: &mut Component) -> bool
where
    Component: ComponentStarter + Sync + Send,
{
    if let Err(err) = component.start().await {
        error!("ComponentServer::start() failed: {:?}", err);
        return false;
    }

    info!("ComponentServer::start() completed.");
    true
}

pub struct RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: for<'a> Deserialize<'a> + Send + 'static,
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
    Request: for<'a> Deserialize<'a> + Send + 'static,
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
    Request: for<'a> Deserialize<'a> + Send + Sync + 'static,
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

pub struct EmptyServer<T: ComponentStarter + Send + Sync> {
    component: T,
}

impl<T: ComponentStarter + Send + Sync> EmptyServer<T> {
    pub fn new(component: T) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<T: ComponentStarter + Send + Sync> ComponentServerStarter for EmptyServer<T> {
    async fn start(&mut self) {
        start_component(&mut self.component).await;
    }
}

pub fn create_empty_server<T: ComponentStarter + Send + Sync>(component: T) -> EmptyServer<T> {
    EmptyServer::new(component)
}
