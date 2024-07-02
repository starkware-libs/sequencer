use std::marker::PhantomData;
use std::net::IpAddr;

use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request as HyperRequest, Uri};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

use crate::component_definitions::{ComponentRequestAndResponseSender, APPLICATION_OCTET_STREAM};

pub struct ComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Request, Response> ComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub fn new(tx: Sender<ComponentRequestAndResponseSender<Request, Response>>) -> Self {
        Self { tx }
    }

    // TODO(Tsabary, 1/5/2024): Consider implementation for messages without expected responses.

    pub async fn send(&self, request: Request) -> Response {
        let (res_tx, mut res_rx) = channel::<Response>(1);
        let request_and_res_tx = ComponentRequestAndResponseSender { request, tx: res_tx };
        self.tx.send(request_and_res_tx).await.expect("Outbound connection should be open.");

        res_rx.recv().await.expect("Inbound connection should be open.")
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require transactions to be cloneable.
impl<Request, Response> Clone for ComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

pub struct ComponentClientHttp<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
{
    uri: Uri,
    client: Client<hyper::client::HttpConnector>,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Request, Response> ComponentClientHttp<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
{
    pub fn new(ip_address: IpAddr, port: u16) -> Self {
        let uri = match ip_address {
            IpAddr::V4(ip_address) => format!("http://{}:{}/", ip_address, port).parse().unwrap(),
            IpAddr::V6(ip_address) => format!("http://[{}]:{}/", ip_address, port).parse().unwrap(),
        };
        // TODO(Tsabary): Add a configuration for the maximum number of idle connections.
        // TODO(Tsabary): Add a configuration for "keep-alive" time of idle connections.
        let client =
            Client::builder().http2_only(true).pool_max_idle_per_host(usize::MAX).build_http();
        Self { uri, client, _req: PhantomData, _res: PhantomData }
    }

    pub async fn send(&self, component_request: Request) -> ClientResult<Response> {
        let http_request = HyperRequest::post(self.uri.clone())
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(
                serialize(&component_request).expect("Request serialization should succeed"),
            ))
            .expect("Request building should succeed");

        // Todo(uriel): Add configuration to control number of retries
        let http_response = self
            .client
            .request(http_request)
            .await
            .map_err(|_e| ClientError::CommunicationFailure)?; // Todo(uriel): To be split into multiple errors
        let body_bytes = to_bytes(http_response.into_body())
            .await
            .map_err(|e| ClientError::ResponseParsingFailure(e.to_string()))?;
        Ok(deserialize(&body_bytes).expect("Response deserialization should succeed"))
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require the generic Request and Response types to be cloneable.
impl<Request, Response> Clone for ComponentClientHttp<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
{
    fn clone(&self) -> Self {
        Self {
            uri: self.uri.clone(),
            client: self.client.clone(),
            _req: PhantomData,
            _res: PhantomData,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ClientError {
    // Todo(uriel): Split this error into more fine grained errors and add a dedicated test
    #[error("Could not connect to server")]
    CommunicationFailure,
    #[error("Could not parse the response: {0}")]
    ResponseParsingFailure(String),
    #[error("Got an unexpected response type.")]
    UnexpectedResponse,
}

pub type ClientResult<T> = Result<T, ClientError>;
