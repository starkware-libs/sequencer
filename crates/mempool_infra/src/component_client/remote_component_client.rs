use std::marker::PhantomData;
use std::net::IpAddr;
use std::sync::Arc;

use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Response as HyperResponse, StatusCode, Uri};
use serde::{Deserialize, Serialize};

use super::definitions::{ClientError, ClientResult};
use crate::component_definitions::APPLICATION_OCTET_STREAM;

pub struct RemoteComponentClient<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
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
    Response: for<'a> Deserialize<'a>,
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

    pub async fn send(&self, component_request: Request) -> ClientResult<Response> {
        for _ in 0..self.max_retries {
            let http_request = self.construct_http_request(&component_request);
            let res = self.try_send(http_request).await;
            if res.is_ok() {
                return res;
            }
        }
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

async fn get_response_body<T>(response: HyperResponse<Body>) -> Result<T, ClientError>
where
    T: for<'a> Deserialize<'a>,
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
    Response: for<'a> Deserialize<'a>,
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
