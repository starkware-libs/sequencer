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
use tokio::time::Instant;
use tracing::{error, info, trace, warn};
use validator::Validate;

use crate::component_definitions::{
    ComponentRequestHandler,
    ComponentStarter,
    PrioritizedRequest,
    RequestPriority,
    RequestWrapper,
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

/// The `LocalComponentServer` struct is a generic server that receives requests and returns
/// responses for a specified component, using Tokio mspc channels for asynchronous communication.
pub struct LocalComponentServer<Component, Request, Response>
where
    Request: Send,
    Response: Send,
{
    component: Option<Component>,
    rx: Receiver<RequestWrapper<Request, Response>>,
    metrics: &'static LocalServerMetrics,
    processing_time_warning_threshold_ms: u128,

    normal_priority_request_rx: Option<Receiver<RequestWrapper<Request, Response>>>,
    high_priority_request_rx: Option<Receiver<RequestWrapper<Request, Response>>>,
    normal_priority_request_tx: Sender<RequestWrapper<Request, Response>>,
    high_priority_request_tx: Sender<RequestWrapper<Request, Response>>,
}

impl<Component, Request, Response> LocalComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: Send + Debug + PrioritizedRequest + 'static,
    Response: Send + Debug + 'static,
{
    pub fn new(
        component: Component,
        rx: Receiver<RequestWrapper<Request, Response>>,
        metrics: &'static LocalServerMetrics,
    ) -> Self {
        // TODO(Tsabary):make the channel capacity configurable.
        let (normal_priority_request_tx, normal_priority_request_rx) =
            channel::<RequestWrapper<Request, Response>>(1000);
        let (high_priority_request_tx, high_priority_request_rx) =
            channel::<RequestWrapper<Request, Response>>(1000);

        let processing_time_warning_threshold_ms = 3_000; // TODO(Tsabary): make this configurable.

        Self {
            component: Some(component),
            rx,
            metrics,
            processing_time_warning_threshold_ms,
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
        let processing_time_warning_threshold_ms = self.processing_time_warning_threshold_ms;
        RequestProcessingMembers {
            component,
            high_rx,
            normal_rx,
            metrics,
            processing_time_warning_threshold_ms,
        }
    }

    async fn await_requests(&mut self) {
        info!(
            "Starting to await requests in the component {} local server",
            short_type_name::<Component>()
        );
        while let Some(request_wrapper) = self.rx.recv().await {
            trace!(
                "Component {} received request {:?} with priority {:?}",
                short_type_name::<Component>(),
                request_wrapper.request,
                request_wrapper.request.priority()
            );
            match request_wrapper.request.priority() {
                RequestPriority::High => {
                    self.high_priority_request_tx
                        .send(request_wrapper)
                        .await
                        .expect("Failed to send high priority request");
                }
                RequestPriority::Normal => {
                    self.normal_priority_request_tx
                        .send(request_wrapper)
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
        let component_name = short_type_name::<Component>();
        info!("Starting to process requests in the component {component_name} local server",);

        let RequestProcessingMembers {
            mut component,
            mut high_rx,
            mut normal_rx,
            metrics,
            processing_time_warning_threshold_ms,
        } = self.get_processing_inner_members();

        tokio::spawn(async move {
            loop {
                let (request, tx) = get_next_request_for_processing(
                    &mut high_rx,
                    &mut normal_rx,
                    &component_name,
                    metrics,
                )
                .await;

                process_request(
                    &mut component,
                    request,
                    tx,
                    metrics,
                    processing_time_warning_threshold_ms,
                )
                .await;
            }
        });
    }
}

impl<Component, Request, Response> Drop for LocalComponentServer<Component, Request, Response>
where
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

/// The `ConcurrentLocalComponentServer` adds a concurrency wrapper to the `LocalComponentServer`,
/// allowing concurrent processing of requests.
pub struct ConcurrentLocalComponentServer<Component, Request, Response>
where
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
        rx: Receiver<RequestWrapper<Request, Response>>,
        max_concurrency: usize,
        metrics: &'static LocalServerMetrics,
    ) -> Self {
        let local_component_server = LocalComponentServer::new(component, rx, metrics);
        Self { local_component_server, max_concurrency }
    }

    async fn await_requests(&mut self) {
        self.local_component_server.await_requests().await;
    }

    async fn process_requests(&mut self) {
        let component_name = short_type_name::<Component>();
        info!(
            "Starting to process requests in the component {component_name} concurrent local \
             server",
        );

        let RequestProcessingMembers {
            component,
            mut high_rx,
            mut normal_rx,
            metrics,
            processing_time_warning_threshold_ms,
        } = self.local_component_server.get_processing_inner_members();

        let task_limiter = Arc::new(Semaphore::new(self.max_concurrency));

        tokio::spawn(async move {
            loop {
                // TODO(Tsabary): add a test for the queueing time metric.
                let (request, tx) = get_next_request_for_processing(
                    &mut high_rx,
                    &mut normal_rx,
                    &component_name,
                    metrics,
                )
                .await;

                // Acquire a permit to run the task.
                let permit = task_limiter.clone().acquire_owned().await.unwrap();

                // Clone the component for concurrent request processing.
                let mut cloned_component = component.clone();
                tokio::spawn(async move {
                    process_request(
                        &mut cloned_component,
                        request,
                        tx,
                        metrics,
                        processing_time_warning_threshold_ms,
                    )
                    .await;
                    // Drop the permit to allow more tasks to be created.
                    drop(permit);
                });
            }
        });
    }
}

impl<Component, Request, Response> Drop
    for ConcurrentLocalComponentServer<Component, Request, Response>
where
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
    metrics: &'static LocalServerMetrics,
    processing_time_warning_threshold_ms: u128,
) where
    Component: ComponentRequestHandler<Request, Response> + Send,
    Request: Send + Debug,
    Response: Send + Debug,
{
    let component_name = short_type_name::<Component>();
    let request_info = format!("{:?}", request);

    trace!("Component {component_name} is starting to process request {request_info:?}",);
    // Please note that the we're measuring the time of an asynchronous request processing, which
    // might also include the awaited time of this task to execute.
    let start = Instant::now();
    let response = component.handle_request(request).await;
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis();
    // TODO(Tsabary): add a test for the processing time metric.
    metrics.record_processing_time(elapsed_ms);

    if elapsed.as_millis() > processing_time_warning_threshold_ms {
        warn!(
            "Component {component_name} took {elapsed_ms} ms to process request {request_info:?}, \
             exceeding the {processing_time_warning_threshold_ms} ms threshold.",
        );
    }

    // TODO(Tsabary): make the processed and received metrics labeled based on the priority and of
    // the request label.
    metrics.increment_processed();

    trace!("Component {component_name} is sending response {response:?}");
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
    high_rx: Receiver<RequestWrapper<Request, Response>>,
    normal_rx: Receiver<RequestWrapper<Request, Response>>,
    metrics: &'static LocalServerMetrics,
    processing_time_warning_threshold_ms: u128,
}

async fn get_next_request_for_processing<Request, Response>(
    high_rx: &mut Receiver<RequestWrapper<Request, Response>>,
    normal_rx: &mut Receiver<RequestWrapper<Request, Response>>,
    component_name: &str,
    metrics: &'static LocalServerMetrics,
) -> (Request, Sender<Response>)
where
    Request: Send + Debug,
    Response: Send,
{
    let request_wrapper = tokio::select! {
        // Prioritize high priority requests over normal priority ones using `biased`.
        biased;
        Some(item) = high_rx.recv() => item,
        Some(item) = normal_rx.recv() => item,
        else => {
            panic!("Stopped processing requests in the component {component_name} local server");
        }
    };
    let request = request_wrapper.request;
    let tx = request_wrapper.tx;
    let creation_time = request_wrapper.creation_time;

    trace!(
        "Component {component_name} received request {request:?} that was created at \
         {creation_time:?}",
    );
    metrics.record_queueing_time(creation_time.elapsed().as_millis());

    (request, tx)
}
