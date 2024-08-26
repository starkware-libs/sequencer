use std::marker::PhantomData;
use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::definitions::{ClientError, ClientResult};
use super::ClientMonitorApi;
use crate::component_definitions::{
    RequestAndMonitor,
    ResponseAndMonitor,
    ServerError,
    APPLICATION_OCTET_STREAM,
};

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
///     let client = RemoteComponentClient::<MyRequest, MyResponse>::new(ip_address, port, 2);
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
    _req: PhantomData<RequestAndMonitor<Request>>,
    _res: PhantomData<ResponseAndMonitor<Response>>,
}

impl<Request, Response> RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: DeserializeOwned,
{
    pub fn new(ip_address: IpAddr, port: u16, max_retries: usize) -> Self {
        let uri = match ip_address {
            IpAddr::V4(ip_address) => format!("http://{}:{}/", ip_address, port).parse().unwrap(),
            IpAddr::V6(ip_address) => format!("http://[{}]:{}/", ip_address, port).parse().unwrap(),
        };
        // TODO(Tsabary): Add a configuration for the maximum number of idle connections.
        // TODO(Tsabary): Add a configuration for "keep-alive" time of idle connections.
        let client =
            Client::builder().http2_only(true).pool_max_idle_per_host(usize::MAX).build_http();
        Self { uri, client, max_retries, _req: PhantomData, _res: PhantomData }
    }

    async fn internal_send(
        &self,
        component_request: RequestAndMonitor<Request>,
    ) -> ClientResult<ResponseAndMonitor<Response>> {
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

    pub async fn send(&self, component_request: Request) -> ClientResult<Response> {
        let requst = RequestAndMonitor::Original(component_request);
        let response = self.internal_send(requst).await?;
        match response {
            ResponseAndMonitor::Original(response) => Ok(response),
            _ => Err(ClientError::UnexpectedResponse("Unexpected response variant.".to_owned())),
        }
    }

    pub async fn send_alive(&self) -> bool {
        let requst = RequestAndMonitor::IsAlive;
        let response = self.internal_send(requst).await;
        match response {
            Ok(ResponseAndMonitor::IsAlive(is_alive)) => is_alive,
            _ => panic!("Unexpected response variant."),
        }
    }

    fn construct_http_request(
        &self,
        component_request: &RequestAndMonitor<Request>,
    ) -> HyperRequest<Body> {
        HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(
                serialize(component_request).expect("Request serialization should succeed"),
            ))
            .expect("Request building should succeed")
    }

    async fn try_send(
        &self,
        http_request: HyperRequest<Body>,
    ) -> ClientResult<ResponseAndMonitor<Response>> {
        let http_response = self
            .client
            .request(http_request)
            .await
            .map_err(|e| ClientError::CommunicationFailure(Arc::new(e)))?;

        match http_response.status() {
            StatusCode::OK => {
                get_response_body::<ResponseAndMonitor<Response>>(http_response).await
            }
            status_code => Err(ClientError::ResponseError(
                status_code,
                get_response_body::<ServerError>(http_response).await?,
            )),
        }
    }
}

async fn get_response_body<T>(response: HyperResponse<Body>) -> ClientResult<T>
where
    T: DeserializeOwned,
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

#[async_trait]
impl<Request, Response> ClientMonitorApi for RemoteComponentClient<Request, Response>
where
    Request: Serialize + Send + Sync,
    Response: DeserializeOwned + Send + Sync,
{
    async fn is_alive(&self) -> bool {
        self.send_alive().await
    }
}
