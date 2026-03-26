use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::watch::Receiver;
use tokio::time::Instant;
use tracing::field::{display, Empty};
use tracing::instrument;

use crate::component_client::ClientResult;
use crate::component_definitions::{
    ComponentClient,
    ComponentClientWithChannel,
    RequestId,
    RequestWrapper,
};
use crate::metrics::LocalClientMetrics;
use crate::requests::LabeledRequest;

/// The `LocalComponentClient` struct is a generic client for sending component requests and
/// receiving responses asynchronously using Tokio mspc channels.
pub struct LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    tx: Sender<RequestWrapper<Request, Response>>,
    metrics: &'static LocalClientMetrics,
}

impl<Request, Response> LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    pub fn new(
        tx: Sender<RequestWrapper<Request, Response>>,
        metrics: &'static LocalClientMetrics,
    ) -> Self {
        Self { tx, metrics }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for LocalComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned + LabeledRequest,
    Response: Send + Serialize + DeserializeOwned,
{
    #[instrument(skip_all, fields(request_id = Empty))]
    async fn send(&self, request: Request) -> ClientResult<Response> {
        let request_id = RequestId::generate();
        tracing::Span::current().record("request_id", display(&request_id));
        let request_label = request.request_label();
        let (res_tx, mut res_rx) = channel::<Response>(1);
        let request_wrapper = RequestWrapper::new(request, res_tx, request_id);
        let start = Instant::now();
        self.tx.send(request_wrapper).await.expect("Outbound connection should be open.");
        let response = res_rx.recv().await.expect("Inbound connection should be open.");
        let elapsed = start.elapsed();
        self.metrics.record_response_time(elapsed.as_secs_f64(), request_label);
        Ok(response)
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require transactions to be cloneable.
impl<Request, Response> Clone for LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), metrics: self.metrics }
    }
}

#[derive(Clone)]
pub struct LocalComponentClientWithChannel<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    info_source_rx: Receiver<InfoSource>,
}

impl<InfoSource> LocalComponentClientWithChannel<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    pub fn new(info_source_rx: Receiver<InfoSource>) -> Self {
        Self { info_source_rx }
    }
}

impl<InfoSource> ComponentClientWithChannel<InfoSource>
    for LocalComponentClientWithChannel<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    fn get_info(&self) -> InfoSource {
        // `borrow()` returns a reference to the value owned by the channel, hence we clone it.
        self.info_source_rx.borrow().clone()
    }
}
