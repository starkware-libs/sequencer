use std::collections::BTreeMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use hyper::body::{to_bytes, Bytes};
use hyper::client::HttpConnector;
use hyper::header::CONTENT_TYPE;
use hyper::{
    Body,
    Client as HyperClient,
    Request as HyperRequest,
    Response as HyperResponse,
    StatusCode,
    Uri,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, trace, warn};
use validator::Validate;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::{ComponentClient, ServerError, APPLICATION_OCTET_STREAM};
use crate::metrics::RemoteClientMetrics;
use crate::serde_utils::SerdeWrapper;

const DEFAULT_CLIENT_COUNT: usize = 10;
const DEFAULT_HTTP2_KEEP_ALIVE_INTERVAL_MS: u64 = 30_000;
const DEFAULT_HTTP2_KEEP_ALIVE_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_POOL_IDLE_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 100;
const DEFAULT_RETRIES: usize = 150;
const DEFAULT_RETRY_INTERVAL_MS: u64 = 1_000;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteClientConfig {
    pub client_count: usize,
    pub http2_keep_alive_interval_ms: u64,
    pub http2_keep_alive_timeout_ms: u64,
    pub pool_idle_timeout_ms: u64,
    pub pool_max_idle_per_host: usize,
    pub retries: usize,
    pub retry_interval_ms: u64,
}

impl Default for RemoteClientConfig {
    fn default() -> Self {
        Self {
            client_count: DEFAULT_CLIENT_COUNT,
            http2_keep_alive_interval_ms: DEFAULT_HTTP2_KEEP_ALIVE_INTERVAL_MS,
            http2_keep_alive_timeout_ms: DEFAULT_HTTP2_KEEP_ALIVE_TIMEOUT_MS,
            pool_idle_timeout_ms: DEFAULT_POOL_IDLE_TIMEOUT_MS,
            pool_max_idle_per_host: DEFAULT_POOL_MAX_IDLE_PER_HOST,
            retries: DEFAULT_RETRIES,
            retry_interval_ms: DEFAULT_RETRY_INTERVAL_MS,
        }
    }
}

// TODO(Tsabary): fill in descriptions.
impl SerializeConfig for RemoteClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "client_count",
                &self.client_count,
                "Number of independent http clients to build. Each has its own connection pool.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "http2_keep_alive_interval_ms",
                &self.http2_keep_alive_interval_ms,
                "HTTP/2 ping interval (ms) for idle connections; detects half-open TCP sessions \
                 and keeps NAT/firewall mappings alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "http2_keep_alive_timeout_ms",
                &self.http2_keep_alive_timeout_ms,
                "Max wait (ms) for a ping ACK before closing the HTTP/2 connection.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "pool_idle_timeout_ms",
                &self.pool_idle_timeout_ms,
                "Idle connection lifetime (ms) before being dropped from the pool.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "pool_max_idle_per_host",
                &self.pool_max_idle_per_host,
                "Max idle connections per host that will be kept pooled.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retries",
                &self.retries,
                "Total retry attempts after the first send.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retry_interval_ms",
                &self.retry_interval_ms,
                "Upper bound for exponential backoff delay (ms) between retries.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// The `RemoteComponentClient` struct is a generic client for sending component requests and
/// receiving responses asynchronously through HTTP connection.
pub struct RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    clients: Arc<Vec<HyperClient<HttpConnector, Body>>>, // Independent connection pools
    config: RemoteClientConfig,
    metrics: RemoteClientMetrics,
    rr: AtomicUsize, // Round-robin index for connection pool selection
    uri: Uri,
    // [`RemoteComponentClient<Request,Response>`] should be [`Send + Sync`] while [`Request`] and
    // [`Response`] are only [`Send`]. [`Phantom<T>`] is [`Send + Sync`] only if [`T`] is, despite
    // this bound making no sense as the phantom data field is unused. As such, we wrap it as
    // [`PhantomData<Mutex<T>>`], not enforcing the redundant [`Sync`] bound. Alternatively,
    // we could also use [`unsafe impl Sync for RemoteComponentClient<Request, Response> {}`], but
    // we prefer the former for the sake of avoiding unsafe code.
    _req: PhantomData<Mutex<Request>>,
    _res: PhantomData<Mutex<Response>>,
}

impl<Request, Response> RemoteComponentClient<Request, Response>
where
    Request: Serialize + DeserializeOwned + Debug,
    Response: Serialize + DeserializeOwned + Debug,
{
    pub fn new(
        config: RemoteClientConfig,
        url: &str,
        port: u16,
        metrics: RemoteClientMetrics,
    ) -> Self {
        let uri = format!("http://{url}:{port}/").parse().unwrap();
        let clients = (0..config.client_count)
            .map(|_| {
                let http = HttpConnector::new();
                HyperClient::builder()
                    .http2_only(true)
                    // TODO(Tsabary): consider making these configurable.
                    .http2_keep_alive_interval(Some(Duration::from_secs(30)))
                    // TODO(Tsabary): consider making these configurable.
                    .http2_keep_alive_timeout(Duration::from_secs(10))
                    .http2_adaptive_window(true)
                    .pool_idle_timeout(Duration::from_millis(config.pool_idle_timeout_ms))
                    .pool_max_idle_per_host(config.pool_max_idle_per_host)
                    .build(http)
            })
            .collect::<Vec<_>>();

        debug!("RemoteComponentClient created with URI: {uri:?}");
        Self {
            clients: Arc::new(clients),
            config,
            metrics,
            rr: AtomicUsize::new(0),
            uri,
            _req: PhantomData,
            _res: PhantomData,
        }
    }

    #[inline]
    fn pick_client(&self) -> &HyperClient<HttpConnector, Body> {
        let i = self.rr.fetch_add(1, Ordering::Relaxed);
        &self.clients[i % self.clients.len()]
    }

    fn construct_http_request(&self, serialized_request: Bytes) -> HyperRequest<Body> {
        trace!("Constructing remote request");
        HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(serialized_request))
            .expect("Request building should succeed")
    }

    async fn try_send(&self, http_request: HyperRequest<Body>) -> ClientResult<Response> {
        trace!("Sending HTTP request");
        let client = self.pick_client();
        let http_response = client.request(http_request).await.map_err(|err| {
            warn!("HTTP request to {} failed with error: {err:?}", self.uri);
            ClientError::CommunicationFailure(err.to_string())
        })?;

        match http_response.status() {
            StatusCode::OK => {
                let response_body = get_response_body(http_response).await;
                trace!("Successfully deserialized response");
                response_body
            }
            status_code => {
                warn!(
                    "Unexpected response status: {status_code:?}. Unable to deserialize response."
                );
                Err(ClientError::ResponseError(
                    status_code,
                    ServerError::RequestDeserializationFailure(
                        "Could not deserialize server response".to_string(),
                    ),
                ))
            }
        }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for RemoteComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned + Debug + AsRef<str>,
    Response: Send + Serialize + DeserializeOwned + Debug,
{
    async fn send(&self, component_request: Request) -> ClientResult<Response> {
        let log_message = format!("{} to {}", component_request.as_ref(), self.uri);

        // Serialize the request.
        let serialized_request = SerdeWrapper::new(component_request)
            .wrapper_serialize()
            .expect("Request serialization should succeed");
        // Convert the serialized request into `Bytes`, a zero-copy, reference-counted buffer used
        // by Hyper. Constructing a Hyper request consumes the body, so we need a way to
        // reuse the request payload across multiple retries without reallocating memory. By
        // using `Bytes` and cloning it per attempt, we preserve the original data
        // efficiently and avoid unnecessary memory copies.
        let serialized_request_bytes: Bytes = serialized_request.into();

        // Construct the request, and send it up to 'max_retries + 1' times. Return if received a
        // successful response, or the last response if all attempts failed.
        let max_attempts = self.config.retries + 1;
        trace!("Starting retry loop: max_attempts = {max_attempts}");
        // TODO(Tsabary): consider making these consts configurable.
        const LOG_ATTEMPT_INTERVAL: usize = 10;
        const INITIAL_RETRY_DELAY: u64 = 1;
        let mut retry_interval_ms = INITIAL_RETRY_DELAY;
        for attempt in 1..max_attempts + 1 {
            trace!("Request {log_message} attempt {attempt} of {max_attempts}");
            let http_request = self.construct_http_request(serialized_request_bytes.clone());
            let res = self.try_send(http_request).await;
            if res.is_ok() {
                trace!("Request {log_message} successful on attempt {attempt}/{max_attempts}");
                self.metrics.record_attempt(attempt);
                return res;
            }
            if attempt % LOG_ATTEMPT_INTERVAL == LOG_ATTEMPT_INTERVAL - 1 {
                warn!("Request {log_message} failed on attempt {attempt}/{max_attempts}: {res:?}");
            }
            if attempt == max_attempts {
                self.metrics.record_attempt(attempt);
                return res;
            }
            tokio::time::sleep(Duration::from_millis(retry_interval_ms)).await;
            // Exponential backoff, capped by the configured retry interval.
            // TODO(Tsabary): rename the config value to indicate this is the max retry interval.
            retry_interval_ms = (retry_interval_ms * 2).min(self.config.retry_interval_ms);
        }
        unreachable!("Guaranteed to return a response before reaching this point.");
    }
}

async fn get_response_body<Response>(response: HyperResponse<Body>) -> Result<Response, ClientError>
where
    Response: Serialize + DeserializeOwned + Debug,
{
    let body_bytes = to_bytes(response.into_body())
        .await
        .map_err(|err| ClientError::ResponseParsingFailure(err.to_string()))?;

    SerdeWrapper::<Response>::wrapper_deserialize(&body_bytes)
        .map_err(|err| ClientError::ResponseDeserializationFailure(err.to_string()))
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require the generic Request and Response types to be cloneable.
impl<Request, Response> Clone for RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    fn clone(&self) -> Self {
        Self {
            uri: self.uri.clone(),
            clients: self.clients.clone(),
            rr: AtomicUsize::new(self.rr.load(Ordering::Relaxed)),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            _req: PhantomData,
            _res: PhantomData,
        }
    }
}
