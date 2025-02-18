use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_infra_utils::type_name::short_type_name;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::{ComponentReplacer, ComponentServerStarter};
use crate::errors::ReplaceComponentError;
use crate::metrics::LocalServerMetrics;

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
    async fn start(&mut self) {
        info!("Starting LocalComponentServer for {}.", short_type_name::<Component>());
        self.component.start().await.unwrap_or_else(|_| {
            panic!("LocalComponentServer stopped for {}", short_type_name::<Component>())
        });
        request_response_loop(&mut self.rx, &mut self.component, self.metrics.clone()).await;
        panic!("Finished LocalComponentServer for {}.", short_type_name::<Component>());
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
    async fn start(&mut self) {
        info!("Starting ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
        self.component.start().await.unwrap_or_else(|_| {
            panic!("ConcurrentLocalComponentServer stopped for {}", short_type_name::<Component>())
        });
        concurrent_request_response_loop(
            &mut self.rx,
            &mut self.component,
            self.max_concurrency,
            self.metrics.clone(),
        )
        .await;
        panic!("Finished ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
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
    // TODO(Itay, Lev): find the way to provide max_concurrency only for non-blocking server.
    max_concurrency: usize,
    metrics: Arc<LocalServerMetrics>,
    _local_server_type: PhantomData<LocalServerType>,
}

// TODO(Itay, Lev): separate the base struct into two distinguished blocking and non blocking
// servers, and modify their constructors accordingly.
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
        max_concurrency: usize,
        metrics: LocalServerMetrics,
    ) -> Self {
        metrics.register();
        Self {
            component,
            rx,
            max_concurrency,
            metrics: Arc::new(metrics),
            _local_server_type: PhantomData,
        }
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
    metrics: Arc<LocalServerMetrics>,
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

        metrics.increment_received();
        metrics.set_queue_depth(rx.len());

        process_request(component, request, tx).await;

        metrics.increment_processed();
    }

    error!("Stopping server for component {}", short_type_name::<Component>());
}

// TODO(Itay): clean some code duplications here.
async fn concurrent_request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
    max_concurrency: usize,
    metrics: Arc<LocalServerMetrics>,
) where
    Component: ComponentRequestHandler<Request, Response> + Clone + Send + 'static,
    Request: Send + Debug + 'static,
    Response: Send + Debug + 'static,
{
    info!("Starting concurrent server for component {}", short_type_name::<Component>());

    let task_limiter = Arc::new(Semaphore::new(max_concurrency));

    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;
        debug!("Component {} received request {:?}", short_type_name::<Component>(), request);

        metrics.increment_received();
        metrics.set_queue_depth(rx.len());

        // Acquire a permit to run the task.
        let permit = task_limiter.clone().acquire_owned().await.unwrap();

        let mut cloned_component = component.clone();
        let cloned_metrics = metrics.clone();
        tokio::spawn(async move {
            process_request(&mut cloned_component, request, tx).await;

            cloned_metrics.increment_processed();

            // Drop the permit to allow more tasks to be created.
            drop(permit);
        });
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
