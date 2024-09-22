use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Sender};

use super::ClientMonitorApi;
use crate::component_definitions::{
    ComponentRequestAndResponseSender,
    MonitoringRequest,
    MonitoringResponse,
    RequestAndMonitor,
    ResponseAndMonitor,
};

/// The `LocalComponentClient` struct is a generic client for sending component requests and
/// receiving responses asynchronously.
///
/// # Type Parameters
/// - `Request`: The type of the request. This type must implement both `Send` and `Sync` traits.
/// - `Response`: The type of the response. This type must implement both `Send` and `Sync` traits.
///
/// # Fields
/// - `tx`: An asynchronous sender channel for transmitting
///   `ComponentRequestAndResponseSender<Request, Response>` messages.
///
/// # Example
/// ```rust
/// // Example usage of the LocalComponentClient
/// use tokio::sync::mpsc::Sender;
///
/// use crate::starknet_mempool_infra::component_client::LocalComponentClient;
/// use crate::starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
///
/// // Define your request and response types
/// struct MyRequest {
///     pub content: String,
/// }
///
/// struct MyResponse {
///     content: String,
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // Create a channel for sending requests and receiving responses
///     let (tx, _rx) = tokio::sync::mpsc::channel::<
///         ComponentRequestAndResponseSender<MyRequest, MyResponse>,
///     >(100);
///
///     // Instantiate the client.
///     let client = LocalComponentClient::new(tx);
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
/// - The `LocalComponentClient` struct is designed to work in an asynchronous environment,
///   utilizing Tokio's async runtime and channels.

pub struct LocalComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Request, Response> LocalComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub fn new(tx: Sender<ComponentRequestAndResponseSender<Request, Response>>) -> Self {
        Self { tx }
    }

    // TODO(Tsabary, 1/5/2024): Consider implementation for messages without expected responses.

    async fn internal_send(
        &self,
        request: RequestAndMonitor<Request>,
    ) -> ResponseAndMonitor<Response> {
        let (res_tx, mut res_rx) = channel::<ResponseAndMonitor<Response>>(1);
        let request_and_res_tx = ComponentRequestAndResponseSender { request, tx: res_tx };
        self.tx.send(request_and_res_tx).await.expect("Outbound connection should be open.");

        res_rx.recv().await.expect("Inbound connection should be open.")
    }

    pub async fn send(&self, request: Request) -> Response {
        let request = RequestAndMonitor::Component(request);
        let res = self.internal_send(request).await;
        match res {
            ResponseAndMonitor::Component(response) => response,
            _ => panic!("Unexpected response variant."),
        }
    }

    pub async fn send_alive(&self) -> bool {
        let request = RequestAndMonitor::Monitoring(MonitoringRequest::IsAlive);
        let res = self.internal_send(request).await;
        match res {
            ResponseAndMonitor::Monitoring(MonitoringResponse::IsAlive(is_alive)) => is_alive,
            _ => panic!("Unexpected response variant."),
        }
    }
}

#[async_trait]
impl<Request, Response> ClientMonitorApi for LocalComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    async fn is_alive(&self) -> bool {
        self.send_alive().await
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require transactions to be cloneable.
impl<Request, Response> Clone for LocalComponentClient<Request, Response>
where
    Request: Send + Sync,
    Response: Send + Sync,
{
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}
