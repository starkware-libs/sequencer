use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::{
    ComponentClient,
    RemoteClientConfig,
    ServerError,
    APPLICATION_OCTET_STREAM,
};
use crate::serde_utils::SerdeWrapper;

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
/// use serde::{Deserialize, Serialize};
///
/// use crate::starknet_sequencer_infra::component_client::RemoteComponentClient;
/// use crate::starknet_sequencer_infra::component_definitions::{
///     ComponentClient,
///     RemoteClientConfig,
/// };
///
/// // Define your request and response types
/// #[derive(Serialize, Deserialize, Debug)]
/// struct MyRequest {
///     pub content: String,
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
///     let config =
///         RemoteClientConfig { retries: 3, idle_connections: usize::MAX, idle_timeout: 90 };
///     let client = RemoteComponentClient::<MyRequest, MyResponse>::new(config, &url, port);
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
    pub fn new(config: RemoteClientConfig, url: &str, port: u16) -> Self {
        let uri = format!("http://{}:{}/", url, port).parse().unwrap();
        let client = Client::builder()
            .http2_only(true)
            .pool_max_idle_per_host(config.idle_connections)
            .pool_idle_timeout(Duration::from_secs(config.idle_timeout))
            .build_http();
        Self { uri, client, config, _req: PhantomData, _res: PhantomData }
    }

    fn construct_http_request(&self, serialized_request: Vec<u8>) -> HyperRequest<Body> {
        HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(serialized_request))
            .expect("Request building should succeed")
    }

    async fn try_send(&self, http_request: HyperRequest<Body>) -> ClientResult<Response> {
        let http_response = self
            .client
            .request(http_request)
            .await
            .map_err(|err| ClientError::CommunicationFailure(err.to_string()))?;

        match http_response.status() {
            StatusCode::OK => get_response_body(http_response).await,
            status_code => Err(ClientError::ResponseError(
                status_code,
                ServerError::RequestDeserializationFailure(
                    "Could not deserialize server response".to_string(),
                ),
            )),
        }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for RemoteComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned + Debug,
    Response: Send + Serialize + DeserializeOwned + Debug,
{
    async fn send(&self, component_request: Request) -> ClientResult<Response> {
        // Serialize the request.
        let serialized_request = SerdeWrapper::new(component_request)
            .wrapper_serialize()
            .expect("Request serialization should succeed");

        // Construct the request, and send it up to 'max_retries + 1' times. Return if received a
        // successful response, or the last response if all attempts failed.
        let max_attempts = self.config.retries + 1;
        for attempt in 0..max_attempts {
            let http_request = self.construct_http_request(serialized_request.clone());
            let res = self.try_send(http_request).await;
            if res.is_ok() {
                return res;
            }
            if attempt == max_attempts - 1 {
                return res;
            }
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
            _req: PhantomData,
            _res: PhantomData,
        }
    }
}
