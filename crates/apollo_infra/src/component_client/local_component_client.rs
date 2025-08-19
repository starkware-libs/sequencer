use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::Instant;

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentClient, RequestWrapper};
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
    async fn send(&self, request: Request) -> ClientResult<Response> {
        let request_label = request.request_label();
        let (res_tx, mut res_rx) = channel::<Response>(1);
        let request_wrapper = RequestWrapper::new(request, res_tx);
        // should sending the request be included in the response time?
        self.tx.send(request_wrapper).await.expect("Outbound connection should be open.");
        let start = Instant::now();
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
