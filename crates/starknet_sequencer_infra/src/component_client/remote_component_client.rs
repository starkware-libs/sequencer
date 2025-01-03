use std::fmt::Debug;
use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::{
    ComponentClient,
    RemoteClientConfig,
    ServerError,
    APPLICATION_OCTET_STREAM,
};
use crate::serde_utils::SerdeWrapper;

/// The `RemoteComponentClient` struct is a client for sending requests and
/// receiving responses asynchronously through HTTP connection.
///
/// # Fields
/// - `uri`: Server URI address.
/// - `client`: Inner HTTP client.
/// - `config`: Client configuration.
///
/// # Example
/// ```rust
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
///     let client = RemoteComponentClient::new(config);
///
///     // Instantiate a request.
///     let request = MyRequest { content: "Hello, world!".to_string() };
///
///     // Send the request; typically, the client should await for a response.
///     client.send(request);
/// }
/// ```
#[derive(Clone)]
pub struct RemoteComponentClient {
    uri: Uri,
    client: Client<hyper::client::HttpConnector>,
    config: RemoteClientConfig,
}

impl RemoteComponentClient {
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
        Self { uri, client, config }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response> for RemoteComponentClient
where
    Request: Send + Serialize + DeserializeOwned + Debug + 'static,
    Response: Send + Serialize + DeserializeOwned + Debug,
{
    async fn send(&self, component_request: Request) -> ClientResult<Response> {
        // Serialize the request.
        let serialized_request = SerdeWrapper::<Request>::new(component_request)
            .wrapper_serialize()
            .expect("Request serialization should succeed");

        // Construct the request, and send it up to 'max_retries + 1' times. Return if received a
        // successful response, or the last response if all attempts failed.
        let max_attempts = self.config.retries + 1;
        for attempt in 0..max_attempts {
            let http_request = construct_http_request(&self.uri, serialized_request.clone());
            let res = try_send(self.client.clone(), http_request).await;
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

async fn try_send<Response>(
    client: Client<hyper::client::HttpConnector>,
    http_request: HyperRequest<Body>,
) -> ClientResult<Response>
where
    Response: Send + Serialize + DeserializeOwned + Debug,
{
    let http_response = client
        .request(http_request)
        .await
        .map_err(|err| ClientError::CommunicationFailure(err.to_string()))?;

    match http_response.status() {
        StatusCode::OK => get_response_body::<Response>(http_response).await,
        status_code => Err(ClientError::ResponseError(
            status_code,
            ServerError::RequestDeserializationFailure(
                "Could not deserialize server response".to_string(),
            ),
        )),
    }
}

fn construct_http_request(uri: &Uri, serialized_request: Vec<u8>) -> HyperRequest<Body> {
    HyperRequest::post(uri)
        .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
        .body(Body::from(serialized_request))
        .expect("Request building should succeed")
}
