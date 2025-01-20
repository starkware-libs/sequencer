use std::fmt::Debug;
use std::marker::PhantomData;

use async_trait::async_trait;
use starknet_infra_utils::type_name::short_type_name;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{debug, error, info, warn};

use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::{ComponentReplacer, ComponentServerStarter};
use crate::errors::{ComponentServerError, ReplaceComponentError};

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
///   `Send` trait to ensure safe concurrency.
/// - `Response`: The type of responses that the component will generate. This type must implement
///   the `Send` trait to ensure safe concurrency.
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
/// use tokio::task;
///
/// use crate::starknet_sequencer_infra::component_definitions::{
///     ComponentRequestAndResponseSender,
///     ComponentRequestHandler,
///     ComponentStarter,
/// };
/// use crate::starknet_sequencer_infra::component_server::{
///     ComponentServerStarter,
///     LocalComponentServer,
/// };
/// use crate::starknet_sequencer_infra::errors::ComponentServerError;
///
/// // Define your component
/// struct MyComponent {}
///
/// impl ComponentStarter for MyComponent {}
///
/// // Define your request and response types
/// #[derive(Debug)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// #[derive(Debug)]
/// struct MyResponse {
///     pub content: String,
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
pub type LocalComponentServer<Component, Request, Response> =
    BaseLocalComponentServer<Component, Request, Response, BlockingLocalServerType>;
pub struct BlockingLocalServerType {}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + ComponentStarter,
    Request: Send + Debug,
    Response: Send + Debug,
{
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        info!("Starting LocalComponentServer for {}.", short_type_name::<Component>());
        self.component.start().await?;
        request_response_loop(&mut self.rx, &mut self.component).await;
        error!("Finished LocalComponentServer for {}.", short_type_name::<Component>());
        Err(ComponentServerError::ServerUnexpectedlyStopped)
    }
}

pub type ConcurrentLocalComponentServer<Component, Request, Response> =
    BaseLocalComponentServer<Component, Request, Response, NonBlockingLocalServerType>;
pub struct NonBlockingLocalServerType {}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for ConcurrentLocalComponentServer<Component, Request, Response>
where
    Component:
        ComponentRequestHandler<Request, Response> + ComponentStarter + Clone + Send + 'static,
    Request: Send + Debug + 'static,
    Response: Send + Debug + 'static,
{
    async fn start(&mut self) -> Result<(), ComponentServerError> {
        info!("Starting ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
        self.component.start().await?;
        concurrent_request_response_loop(&mut self.rx, &mut self.component).await;
        error!("Finished ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
        Err(ComponentServerError::ServerUnexpectedlyStopped)
    }
}

pub struct BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    _local_server_type: PhantomData<LocalServerType>,
}

impl<Component, Request, Response, LocalServerType>
    BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    ) -> Self {
        Self { component, rx, _local_server_type: PhantomData }
    }
}

impl<Component, Request, Response, LocalServerType> ComponentReplacer<Component>
    for BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError> {
        self.component = component;
        Ok(())
    }
}

impl<Component, Request, Response, LocalServerType> Drop
    for BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    fn drop(&mut self) {
        warn!("Dropping {}.", short_type_name::<Self>());
    }
}

async fn request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
) where
    Component: ComponentRequestHandler<Request, Response> + Send,
    Request: Send + Debug,
    Response: Send + Debug,
{
    info!("Starting server for component {}", short_type_name::<Component>());

    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;
        debug!("Component {} received request {:?}", short_type_name::<Component>(), request);

        process_request(component, request, tx).await;
    }

    error!("Stopping server for component {}", short_type_name::<Component>());
}

// TODO(Itay): clean some code duplications here.
async fn concurrent_request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
) where
    Component: ComponentRequestHandler<Request, Response> + Clone + Send + 'static,
    Request: Send + Debug + 'static,
    Response: Send + Debug + 'static,
{
    info!("Starting concurrent server for component {}", short_type_name::<Component>());

    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;
        debug!("Component {} received request {:?}", short_type_name::<Component>(), request);

        let mut cloned_component = component.clone();
        tokio::spawn(async move { process_request(&mut cloned_component, request, tx).await });
    }

    error!("Stopping concurrent server for component {}", short_type_name::<Component>());
}

async fn process_request<Request, Response, Component>(
    component: &mut Component,
    request: Request,
    tx: Sender<Response>,
) where
    Component: ComponentRequestHandler<Request, Response> + Send,
    Request: Send + Debug,
    Response: Send + Debug,
{
    let response = component.handle_request(request).await;
    debug!("Component {} is sending response {:?}", short_type_name::<Component>(), response);

    // Send the response to the client. This might result in a panic if the client has closed
    // the response channel, which is considered a bug.
    tx.send(response).await.expect("Response connection should be open.");
}
