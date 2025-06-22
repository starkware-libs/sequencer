use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Semaphore;
use tracing::{error, info, trace, warn};
use validator::Validate;

use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::LocalServerMetrics;

const DEFAULT_CHANNEL_CAPACITY: usize = 128;

// The communication configuration of the local component.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct LocalServerConfig {
    pub channel_capacity: usize,
}

impl SerializeConfig for LocalServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "channel_capacity",
            &self.channel_capacity,
            "The communication channel buffer size.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for LocalServerConfig {
    fn default() -> Self {
        Self { channel_capacity: DEFAULT_CHANNEL_CAPACITY }
    }
}

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
/// - `metrics`: The metrics for the server.
pub struct LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    metrics: LocalServerMetrics,
}

impl<Component, Request, Response> LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
        metrics: LocalServerMetrics,
    ) -> Self {
        metrics.register();
        Self { component, rx, metrics }
    }
}

impl<Component, Request, Response> Drop for LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    fn drop(&mut self) {
        warn!("Dropping {}.", short_type_name::<Self>());
    }
}

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
        self.component.start().await;
        request_response_loop(&mut self.rx, &mut self.component, &self.metrics).await;
        panic!("Finished LocalComponentServer for {}.", short_type_name::<Component>());
    }
}

async fn request_response_loop<Request, Response, Component>(
    rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component: &mut Component,
    metrics: &LocalServerMetrics,
) where
    Component: ComponentRequestHandler<Request, Response> + Send,
    Request: Send + Debug,
    Response: Send + Debug,
{
    info!("Starting server for component {}", short_type_name::<Component>());

    while let Some(request_and_res_tx) = rx.recv().await {
        let request = request_and_res_tx.request;
        let tx = request_and_res_tx.tx;
        trace!("Component {} received request {:?}", short_type_name::<Component>(), request);

        metrics.increment_received();
        metrics.set_queue_depth(rx.len());

        process_request(component, request, tx).await;

        metrics.increment_processed();
    }

    error!("Stopping server for component {}", short_type_name::<Component>());
}

/// The `ConcurrentLocalComponentServer` struct is a generic server that handles concurrent requests
/// and responses for a specified component. It receives requests, processes them concurrently by
/// running the provided component in a task, with returning response back form the task. The server
/// needs to be started using the `start` function, which runs indefinitely.
///
/// # Type Parameters
///
/// - `Component`: The type of the component that will handle the requests. This type must implement
///   the `ComponentRequestHandler` trait, which defines how the component processes requests and
///   generates responses. In order to handle concurrent requests, the component must also implement
///   the `Clone` trait and the `Send`.
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
/// - `max_concurrency`: The maximum number of concurrent requests that the server can handle.
/// - `metrics`: The metrics for the server wrapped in Arc so it could be used concurrently.
pub struct ConcurrentLocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    max_concurrency: usize,
    metrics: Arc<LocalServerMetrics>,
}

impl<Component, Request, Response> ConcurrentLocalComponentServer<Component, Request, Response>
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
        Self { component, rx, max_concurrency, metrics: Arc::new(metrics) }
    }
}

// TODO(Lev,Itay): Find a way to avoid duplicity, maybe by a blanket implementation.
impl<Component, Request, Response> Drop
    for ConcurrentLocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send,
    Response: Send,
{
    fn drop(&mut self) {
        warn!("Dropping {}.", short_type_name::<Self>());
    }
}

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
        self.component.start().await;
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
        trace!("Component {} received request {:?}", short_type_name::<Component>(), request);

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
    trace!("Component {} is sending response {:?}", short_type_name::<Component>(), response);

    // Send the response to the client. This might result in a panic if the client has closed
    // the response channel, which is considered a bug.
    tx.send(response).await.expect("Response connection should be open.");
}
