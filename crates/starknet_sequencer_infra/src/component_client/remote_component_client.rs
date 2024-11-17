use std::fmt::Debug;
use std::marker::PhantomData;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::{ComponentClient, RemoteClientConfig, APPLICATION_OCTET_STREAM};
use crate::serde_utils::BincodeSerdeWrapper;

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
///     let ip_address = std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
///     let port: u16 = 8080;
///     let socket = std::net::SocketAddr::new(ip_address, port);
///     let config = RemoteClientConfig {
///         socket,
///         retries: 3,
///         idle_connections: usize::MAX,
///         idle_timeout: 90,
///     };
///     let client = RemoteComponentClient::<MyRequest, MyResponse>::new(config);
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
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Request, Response> RemoteComponentClient<Request, Response>
where
    Request: Serialize + DeserializeOwned + Debug,
    Response: Serialize + DeserializeOwned + Debug,
{
    pub fn new(config: RemoteClientConfig) -> Self {
        let ip_address = config.socket.ip();
        let port = config.socket.port();
        let uri = match ip_address {
            IpAddr::V4(ip_address) => format!("http://{}:{}/", ip_address, port).parse().unwrap(),
            IpAddr::V6(ip_address) => format!("http://[{}]:{}/", ip_address, port).parse().unwrap(),
        };
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
            .map_err(|e| ClientError::CommunicationFailure(Arc::new(e)))?;

        match http_response.status() {
            StatusCode::OK => get_response_body(http_response).await,
            status_code => Err(ClientError::ResponseError(
                status_code,
                get_response_body(http_response).await?,
            )),
        }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for RemoteComponentClient<Request, Response>
where
    Request: Send + Sync + Serialize + DeserializeOwned + Debug,
    Response: Send + Sync + Serialize + DeserializeOwned + Debug,
{
    async fn send(&self, component_request: Request) -> ClientResult<Response> {
        // Serialize the request.
        let serialized_request = BincodeSerdeWrapper::new(component_request)
            .to_bincode()
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
        .map_err(|e| ClientError::ResponseParsingFailure(Arc::new(e)))?;

    BincodeSerdeWrapper::<Response>::from_bincode(&body_bytes)
        .map_err(|e| ClientError::ResponseDeserializationFailure(Arc::new(e)))
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
