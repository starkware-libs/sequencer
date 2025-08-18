use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc::{channel, Sender};

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentClient, RequestWrapper};
use crate::metrics::LocalClientMetrics;

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
    Request: Send + Serialize + DeserializeOwned,
    Response: Send + Serialize + DeserializeOwned,
{
    async fn send(&self, request: Request) -> ClientResult<Response> {
        let (res_tx, mut res_rx) = channel::<Response>(1);
        let request_wrapper = RequestWrapper::new(request, res_tx);
        self.tx.send(request_wrapper).await.expect("Outbound connection should be open.");
        Ok(res_rx.recv().await.expect("Inbound connection should be open."))
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
