use std::marker::PhantomData;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::APPLICATION_OCTET_STREAM;

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
/// - `max_retries`: Configurable number of extra attempts to send a request to server in case of a
///   failure.
/// - `keep_alive_timeout`: Optional cunfigurable time for specifying how long to keep connections
///   alive. Default is
/// - `max_idle`: Optional configurable number of idle connections the client holds. Default is
///   usize::MAX.
///
/// # Example
/// ```rust
/// // Example usage of the RemoteComponentClient
///
/// use serde::{Deserialize, Serialize};
///
/// use crate::starknet_mempool_infra::component_client::RemoteComponentClient;
///
/// // Define your request and response types
/// #[derive(Serialize)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// #[derive(Deserialize)]
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
///     let client =
///         RemoteComponentClient::<MyRequest, MyResponse>::new(ip_address, port, 2, None, None);
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
///   utilizing Tokio's async runtime and hyper framwork to send HTTP requests and receive HTTP
///   responses.
pub struct RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    uri: Uri,
    client: Client<hyper::client::HttpConnector>,
    max_retries: usize,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Request, Response> RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    pub fn new(
        ip_address: IpAddr,
        port: u16,
        max_retries: usize,
        keep_alive_timeout: Option<Duration>,
        max_idle: Option<usize>,
    ) -> Self {
        let uri = match ip_address {
            IpAddr::V4(ip_address) => format!("http://{}:{}/", ip_address, port).parse().unwrap(),
            IpAddr::V6(ip_address) => format!("http://[{}]:{}/", ip_address, port).parse().unwrap(),
        };

        let mut builder = Client::builder();
        if let Some(keep_alive_timeout) = keep_alive_timeout {
            builder.http2_keep_alive_timeout(keep_alive_timeout);
        }
        let client = builder
            .http2_only(true)
            .pool_max_idle_per_host(max_idle.unwrap_or(usize::MAX))
            .build_http();
        Self { uri, client, max_retries, _req: PhantomData, _res: PhantomData }
    }

    pub async fn send(&self, component_request: Request) -> ClientResult<Response> {
        // Construct and request, and send it up to 'max_retries' times. Return if received a
        // successful response.
        for _ in 0..self.max_retries {
            let http_request = self.construct_http_request(&component_request);
            let res = self.try_send(http_request).await;
            if res.is_ok() {
                return res;
            }
        }
        // Construct and send the request, return the received respone regardless whether it
        // successful or not.
        let http_request = self.construct_http_request(&component_request);
        self.try_send(http_request).await
    }

    fn construct_http_request(&self, component_request: &Request) -> HyperRequest<Body> {
        HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(
                serialize(component_request).expect("Request serialization should succeed"),
            ))
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

async fn get_response_body<Response>(response: HyperResponse<Body>) -> Result<Response, ClientError>
where
    Response: DeserializeOwned,
{
    let body_bytes = to_bytes(response.into_body())
        .await
        .map_err(|e| ClientError::ResponseParsingFailure(Arc::new(e)))?;
    deserialize(&body_bytes).map_err(|e| ClientError::ResponseDeserializationFailure(Arc::new(e)))
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
            max_retries: self.max_retries,
            _req: PhantomData,
            _res: PhantomData,
        }
    }
}
