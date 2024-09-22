use std::marker::PhantomData;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tracing::error;

use super::definitions::{request_response_loop, start_component, ComponentServerStarter};
use crate::component_definitions::{ComponentRequestAndResponseSender, ComponentRequestHandler};
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
///     ComponentRequestAndResponseSender,
///     ComponentRequestHandler,
/// };
/// use crate::starknet_mempool_infra::component_server::{
///     ComponentServerStarter,
///     LocalComponentServer,
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
pub type LocalComponentServer<Component, Request, Response> =
    BaseLocalComponentServer<Component, Request, Response, BlockingLocalServerType>;
pub struct BlockingLocalServerType {}
#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    async fn start(&mut self) {
        self.set_started();

        // TODO(Tsabary/Lev): Find a better mechanism than the if condition to determine what to
        // run.
        if start_component(&mut self.component).await {
            request_response_loop(&mut self.rx, &mut self.component).await;
        }
    }
}

pub type LocalActiveComponentServer<Component, Request, Response> =
    BaseLocalComponentServer<Component, Request, Response, NonBlockingLocalServerType>;
pub struct NonBlockingLocalServerType {}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for LocalActiveComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter + Clone + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    async fn start(&mut self) {
        self.set_started();

        let mut component = self.component.clone();
        let component_future = async move { component.start().await };
        let request_response_future = request_response_loop(&mut self.rx, &mut self.component);

        tokio::select! {
            _res = component_future => {
                error!("Component stopped.");
            }
            _res = request_response_future => {
                error!("Server stopped.");
            }
        };
        error!("Server ended with unexpected Ok.");
    }
}

pub struct BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter,
    Request: Send + Sync,
    Response: Send + Sync,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    started: bool,
    _local_server_type: PhantomData<LocalServerType>,
}

impl<Component, Request, Response, LocalServerType>
    BaseLocalComponentServer<Component, Request, Response, LocalServerType>
where
    Component: ComponentRequestHandler<Request, Response> + ComponentStarter,
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    ) -> Self {
        Self { component, rx, started: false, _local_server_type: PhantomData }
    }

    pub fn replace_component(
        &mut self,
        component: Component,
    ) -> Result<(), ComponentReplacementError> {
        if self.started {
            return Err(ComponentReplacementError::ComponentAlreadyRunning {});
        }
        self.component = component;
        Ok(())
    }

    fn set_started(&mut self) {
        self.started = true;
    }
}

#[derive(Debug, Error)]
pub enum ComponentReplacementError {
    #[error("Component already has been started")]
    ComponentAlreadyRunning {},
}
