use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::Instant;
use tracing::field::{display, Empty};
use tracing::{instrument, warn};
use validator::Validate;

use super::remote_component_client::REQUEST_TIMEOUT_ERROR_MESSAGE;
use crate::component_client::{ClientError, ClientResult};
use crate::component_definitions::{ComponentClient, RequestId, RequestWrapper};
use crate::metrics::LocalClientMetrics;
use crate::requests::LabeledRequest;

const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct LocalClientConfig {
    pub request_timeout_ms: u64,
}

impl Default for LocalClientConfig {
    fn default() -> Self {
        Self { request_timeout_ms: DEFAULT_REQUEST_TIMEOUT_MS }
    }
}

impl SerializeConfig for LocalClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "request_timeout_ms",
            &self.request_timeout_ms,
            "The maximal duration in milliseconds for the full request-response cycle.",
            ParamPrivacyInput::Public,
        )])
    }
}

/// The `LocalComponentClient` struct is a generic client for sending component requests and
/// receiving responses asynchronously using Tokio mspc channels.
pub struct LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    config: LocalClientConfig,
    tx: Sender<RequestWrapper<Request, Response>>,
    metrics: &'static LocalClientMetrics,
}

impl<Request, Response> LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    pub fn new(
        config: LocalClientConfig,
        tx: Sender<RequestWrapper<Request, Response>>,
        metrics: &'static LocalClientMetrics,
    ) -> Self {
        Self { config, tx, metrics }
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
        let timeout_duration = Duration::from_millis(self.config.request_timeout_ms);
        let request_future = async {
            self.tx.send(request_wrapper).await.expect("Outbound connection should be open.");
            res_rx.recv().await.expect("Inbound connection should be open.")
        };
        match tokio::time::timeout(timeout_duration, request_future).await {
            Ok(response) => {
                let elapsed = start.elapsed();
                self.metrics.record_response_time(elapsed.as_secs_f64(), request_label);
                Ok(response)
            }
            Err(_) => {
                warn!("Local request timed out after {} ms", self.config.request_timeout_ms);
                Err(ClientError::CommunicationFailure(REQUEST_TIMEOUT_ERROR_MESSAGE.to_string()))
            }
        }
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
        Self { config: self.config.clone(), tx: self.tx.clone(), metrics: self.metrics }
    }
}
