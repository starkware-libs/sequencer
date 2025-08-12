use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Semaphore;
use tracing::{error, info, trace, warn};
use validator::Validate;

use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
    PrioritizedRequest,
    RequestPriority,
};
use crate::component_server::ComponentServerStarter;
use crate::metrics::LocalServerMetrics;

// TODO(Tsabary): create custom configs per service, considering the required throughput and spike
// tolerance.

const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

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
    component: Option<Component>,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    metrics: &'static LocalServerMetrics,

    normal_priority_request_rx:
        Option<Receiver<ComponentRequestAndResponseSender<Request, Response>>>,
    high_priority_request_rx:
        Option<Receiver<ComponentRequestAndResponseSender<Request, Response>>>,
    normal_priority_request_tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
    high_priority_request_tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Component, Request, Response> LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: Send + Debug + PrioritizedRequest + 'static,
    Response: Send + Debug + 'static,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
        metrics: &'static LocalServerMetrics,
    ) -> Self {
        // TODO(Tsabary):make the channel capacity configurable.
        let (normal_priority_request_tx, normal_priority_request_rx) =
            channel::<ComponentRequestAndResponseSender<Request, Response>>(1000);
        let (high_priority_request_tx, high_priority_request_rx) =
            channel::<ComponentRequestAndResponseSender<Request, Response>>(1000);

        Self {
            component: Some(component),
            rx,
            metrics,
            normal_priority_request_tx,
            normal_priority_request_rx: Some(normal_priority_request_rx),
            high_priority_request_tx,
            high_priority_request_rx: Some(high_priority_request_rx),
        }
    }

    fn get_processing_inner_members(
        &mut self,
    ) -> RequestProcessingMembers<Request, Response, Component> {
        // Take ownership of the component and the priority request receivers, so they can be used
        // in the async task.
        let component = self.component.take().expect("Component should be available");
        let high_rx = self
            .high_priority_request_rx
            .take()
            .expect("High priority request receiver should be available");
        let normal_rx = self
            .normal_priority_request_rx
            .take()
            .expect("Normal priority request receiver should be available");
        let metrics = self.metrics;

        RequestProcessingMembers { component, high_rx, normal_rx, metrics }
    }

    async fn await_requests(&mut self) {
        info!(
            "Starting to await requests in the component {} local server",
            short_type_name::<Component>()
        );
        while let Some(request_and_res_tx) = self.rx.recv().await {
            trace!(
                "Component {} received request {:?} with priority {:?}",
                short_type_name::<Component>(),
                request_and_res_tx.request,
                request_and_res_tx.request.priority()
            );
            match request_and_res_tx.request.priority() {
                RequestPriority::High => {
                    self.high_priority_request_tx
                        .send(request_and_res_tx)
                        .await
                        .expect("Failed to send high priority request");
                }
                RequestPriority::Normal => {
                    self.normal_priority_request_tx
                        .send(request_and_res_tx)
                        .await
                        .expect("Failed to send low priority request");
                }
            }
            self.metrics.increment_received();
            self.metrics.set_queue_depth(self.rx.len());
        }

        error!(
            "Stopped awaiting requests in the component {} local server",
            short_type_name::<Component>()
        );
    }

    async fn process_requests(&mut self) {
        // TODO(Tsabary): add log for requests that take too long.
        let component_name = short_type_name::<Component>();
        info!("Starting to process requests in the component {component_name} local server",);

        let RequestProcessingMembers { mut component, mut high_rx, mut normal_rx, metrics } =
            self.get_processing_inner_members();

        tokio::spawn(async move {
            loop {
                let (request, tx) =
                    get_next_request_for_processing(&mut high_rx, &mut normal_rx, &component_name)
                        .await;

                process_request(&mut component, request, tx).await;
                metrics.increment_processed();
                // TODO(Tsabary): make the processed and received metrics labeled based on the
                // priority.
            }
        });
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
    Component: ComponentRequestHandler<Request, Response> + Send + ComponentStarter + 'static,
    Request: Send + Debug + PrioritizedRequest + 'static,
    Response: Send + Debug + 'static,
{
    async fn start(&mut self) {
        info!("Starting LocalComponentServer for {}.", short_type_name::<Component>());
        self.metrics.register();
        self.component.as_mut().unwrap().start().await;
        self.process_requests().await;
        self.await_requests().await;
        panic!("Finished LocalComponentServer for {}.", short_type_name::<Component>());
    }
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
    local_component_server: LocalComponentServer<Component, Request, Response>,

    max_concurrency: usize,
}

impl<Component, Request, Response> ConcurrentLocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Clone + Send + 'static,
    Request: Send + Debug + PrioritizedRequest + 'static,
    Response: Send + Debug + 'static,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
        max_concurrency: usize,
        metrics: &'static LocalServerMetrics,
    ) -> Self {
        let local_component_server = LocalComponentServer::new(component, rx, metrics);
        Self { local_component_server, max_concurrency }
    }

    async fn await_requests(&mut self) {
        self.local_component_server.await_requests().await;
    }

    // TODO(Tsabary): avoid code duplication with `LocalComponentServer::process_requests`.
    async fn process_requests(&mut self) {
        // TODO(Tsabary): add log for requests that take too long.
        let component_name = short_type_name::<Component>();
        info!(
            "Starting to process requests in the component {component_name} concurrent local \
             server",
        );

        let RequestProcessingMembers { component, mut high_rx, mut normal_rx, metrics } =
            self.local_component_server.get_processing_inner_members();

        let task_limiter = Arc::new(Semaphore::new(self.max_concurrency));

        // TODO(Itay): clean some code duplications here.
        tokio::spawn(async move {
            loop {
                let (request, tx) =
                    get_next_request_for_processing(&mut high_rx, &mut normal_rx, &component_name)
                        .await;

                // Acquire a permit to run the task.
                let permit = task_limiter.clone().acquire_owned().await.unwrap();

                let mut cloned_component = component.clone();
                tokio::spawn(async move {
                    process_request(&mut cloned_component, request, tx).await;

                    metrics.increment_processed();

                    // Drop the permit to allow more tasks to be created.
                    drop(permit);
                    // TODO(Tsabary): make the processed and received metrics labeled based on the
                    // priority.
                });
            }
        });
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
    Request: Send + Debug + PrioritizedRequest + 'static,
    Response: Send + Debug + 'static,
{
    async fn start(&mut self) {
        info!("Starting ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
        self.local_component_server.metrics.register();
        self.local_component_server.component.as_mut().unwrap().start().await;
        self.process_requests().await;
        self.await_requests().await;
        panic!("Finished ConcurrentLocalComponentServer for {}.", short_type_name::<Component>());
    }
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
    trace!(
        "Component {} is starting to process request {:?}",
        short_type_name::<Component>(),
        request
    );
    let response = component.handle_request(request).await;
    trace!("Component {} is sending response {:?}", short_type_name::<Component>(), response);

    // Send the response to the client. This might result in a panic if the client has closed
    // the response channel, which is considered a bug.
    tx.send(response).await.expect("Response connection should be open.");
}

struct RequestProcessingMembers<Request, Response, Component>
where
    Request: Send,
    Response: Send,
{
    component: Component,
    high_rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    normal_rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    metrics: &'static LocalServerMetrics,
}

async fn get_next_request_for_processing<Request, Response>(
    high_rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    normal_rx: &mut Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    component_name: &str,
) -> (Request, Sender<Response>)
where
    Request: Send + Debug,
    Response: Send,
{
    let request_and_res_tx = tokio::select! {
        // Prioritize high priority requests over normal priority ones using `biased`.
        biased;
        Some(item) = high_rx.recv() => item,
        Some(item) = normal_rx.recv() => item,
        else => {
            panic!("Stopped processing requests in the component {component_name} local server");
        }
    };
    let request = request_and_res_tx.request;
    let tx = request_and_res_tx.tx;

    trace!("Component {component_name} received request {request:?}",);
    (request, tx)
}
