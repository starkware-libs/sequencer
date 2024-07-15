use std::marker::PhantomData;
use std::net::IpAddr;

use bincode::{deserialize, serialize, ErrorKind};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{
    Body, Client, Error as HyperError, Request as HyperRequest, Response as HyperResponse,
    StatusCode, Uri,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{channel, Sender};

use crate::component_definitions::{
    ComponentRequestAndResponseSender, ServerError, APPLICATION_OCTET_STREAM,
};

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

        // Todo(uriel): Add configuration for controlling the number of retries.
        let http_response =
            self.client.request(http_request).await.map_err(ClientError::CommunicationFailure)?;

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
    let body_bytes =
        to_bytes(response.into_body()).await.map_err(ClientError::ResponseParsingFailure)?;
    deserialize(&body_bytes).map_err(ClientError::ResponseDeserializationFailure)
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

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Communication error: {0}")]
    CommunicationFailure(HyperError),
    #[error("Could not deserialize server response: {0}")]
    ResponseDeserializationFailure(Box<ErrorKind>),
    #[error("Could not parse the response: {0}")]
    ResponseParsingFailure(HyperError),
    #[error("Got status code: {0}, with server error: {1}")]
    ResponseError(StatusCode, ServerError),
    #[error("Got an unexpected response type.")]
    UnexpectedResponse,
}

pub type ClientResult<T> = Result<T, ClientError>;
