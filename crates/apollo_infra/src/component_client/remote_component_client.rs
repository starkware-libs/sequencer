use std::collections::BTreeMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, trace, warn};
use validator::Validate;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::{ComponentClient, ServerError, APPLICATION_OCTET_STREAM};
use crate::metrics::RemoteClientMetrics;
use crate::serde_utils::SerdeWrapper;

// TODO(Tsabary): rename all constants to better describe their purpose.
const DEFAULT_RETRIES: usize = 150;
const DEFAULT_IDLE_CONNECTIONS: usize = 10;
// TODO(Tsabary): add `_SECS` suffix to the constant names and the config fields.
const DEFAULT_IDLE_TIMEOUT_MS: u64 = 30000;
const DEFAULT_RETRY_INTERVAL_MS: u64 = 1000;

// TODO(Tsabary): consider retry delay mechanisms, e.g., exponential backoff, jitter, etc.

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteClientConfig {
    pub retries: usize,
    pub idle_connections: usize,
    pub idle_timeout_ms: u64,
    pub retry_interval_ms: u64,
}

impl Default for RemoteClientConfig {
    fn default() -> Self {
        Self {
            retries: DEFAULT_RETRIES,
            idle_connections: DEFAULT_IDLE_CONNECTIONS,
            idle_timeout_ms: DEFAULT_IDLE_TIMEOUT_MS,
            retry_interval_ms: DEFAULT_RETRY_INTERVAL_MS,
        }
    }
}

impl SerializeConfig for RemoteClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "retries",
                &self.retries,
                "The max number of retries for sending a message.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connections",
                &self.idle_connections,
                "The maximum number of idle connections to keep alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_timeout_ms",
                &self.idle_timeout_ms,
                "The duration in milliseconds to keep an idle connection open before closing.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retry_interval_ms",
                &self.retry_interval_ms,
                "The duration in milliseconds to wait between remote connection retries.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

/// The `RemoteComponentClient` struct is a generic client for sending component requests and
/// receiving responses asynchronously through HTTP connection.
///
/// # Type Parameters
/// - `Request`: The type of the request. This type must implement the `serde::Serialize` trait.
/// - `Response`: The type of the response. This type must implement the
///   `serde::de::DeserializeOwned` (e.g. by using #[derive(Deserialize)]) trait.
///
/// # Fields
/// - `uri`: URI address of the server.
/// - `client`: The inner HTTP client that initiates the connection to the server and manages it.
/// - `config`: Client configuration.
///
/// # Example
/// ```rust
/// // Example usage of the RemoteComponentClient
///
/// use apollo_infra::metrics::RemoteClientMetrics;
/// use apollo_metrics::metrics::{MetricHistogram, MetricScope};
/// use serde::{Deserialize, Serialize};
///
/// use crate::apollo_infra::component_client::{RemoteClientConfig, RemoteComponentClient};
/// use crate::apollo_infra::component_definitions::ComponentClient;
///
/// // Define your request and response types
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// impl AsRef<str> for MyRequest {
///     fn as_ref(&self) -> &str {
///         &self.content
///     }
/// }
///
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyResponse {
///     content: String,
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // Create a channel for sending requests and receiving responses
///     // Instantiate the client.
///     let url = "127.0.0.1".to_string();
///     let port: u16 = 8080;
///     let config = RemoteClientConfig::default();
///
///     const EXAMPLE_HISTOGRAM_METRIC: MetricHistogram = MetricHistogram::new(
///         MetricScope::Infra,
///         "example_histogram_metric",
///         "example_histogram_metric_filter",
///         "example_histogram_metric_sum_filter",
///         "example_histogram_metric_count_filter",
///         "Example histogram metrics",
///     );
///     let metrics = RemoteClientMetrics::new(&EXAMPLE_HISTOGRAM_METRIC);
///     let client =
///         RemoteComponentClient::<MyRequest, MyResponse>::new(config, &url, port, metrics);
///
///     // Instantiate a request.
///     let request = MyRequest { content: "Hello, world!".to_string() };
///
///     // Send the request; typically, the client should await for a response.
///     client.send(request);
/// }
/// ```
///
/// # Notes
/// - The `RemoteComponentClient` struct is designed to work in an asynchronous environment,
///   utilizing Tokio's async runtime and hyper framework to send HTTP requests and receive HTTP
///   responses.
pub struct RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    uri: Uri,
    client: Client<hyper::client::HttpConnector>,
    config: RemoteClientConfig,
    metrics: RemoteClientMetrics,
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
        let client = Client::builder()
            .http2_only(true)
            .pool_max_idle_per_host(config.idle_connections)
            .pool_idle_timeout(Duration::from_millis(config.idle_timeout_ms))
            .build_http();
        debug!("RemoteComponentClient created with URI: {uri:?}");
        Self { uri, client, config, metrics, _req: PhantomData, _res: PhantomData }
    }

    fn construct_http_request(&self, serialized_request: Vec<u8>) -> HyperRequest<Body> {
        trace!("Constructing remote request");
        HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(serialized_request))
            .expect("Request building should succeed")
    }

    async fn try_send(&self, http_request: HyperRequest<Body>) -> ClientResult<Response> {
        trace!("Sending HTTP request");
        let http_response = self.client.request(http_request).await.map_err(|err| {
            warn!("HTTP request failed with error: {:?}", err);
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

        // Construct the request, and send it up to 'max_retries + 1' times. Return if received a
        // successful response, or the last response if all attempts failed.
        let max_attempts = self.config.retries + 1;
        trace!("Starting retry loop: max_attempts = {max_attempts}");
        for attempt in 1..max_attempts + 1 {
            trace!("Request {log_message} attempt {attempt} of {max_attempts}");
            let http_request = self.construct_http_request(serialized_request.clone());
            let res = self.try_send(http_request).await;
            if res.is_ok() {
                trace!("Request {log_message} successful on attempt {attempt}/{max_attempts}");
                self.metrics.record_attempt(attempt);
                return res;
            }
            warn!("Request {log_message} failed on attempt {attempt}/{max_attempts}: {res:?}");
            if attempt == max_attempts {
                self.metrics.record_attempt(attempt);
                return res;
            }
            tokio::time::sleep(Duration::from_millis(self.config.retry_interval_ms)).await;
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
            client: self.client.clone(),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            _req: PhantomData,
            _res: PhantomData,
        }
    }
}
