use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::mpsc::{channel, Sender};

use crate::component_client::ClientResult;
use crate::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};

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
/// use serde::{Deserialize, Serialize};
/// use tokio::sync::mpsc::Sender;
///
/// use crate::apollo_sequencer_infra::component_client::LocalComponentClient;
/// use crate::apollo_sequencer_infra::component_definitions::{
///     ComponentClient,
///     ComponentRequestAndResponseSender,
/// };
///
/// // Define your request and response types
/// #[derive(Deserialize, Serialize)]
/// struct MyRequest {
///     pub content: String,
/// }
///
/// #[derive(Deserialize, Serialize)]
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
    Request: Send,
    Response: Send,
{
    tx: Sender<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Request, Response> LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    pub fn new(tx: Sender<ComponentRequestAndResponseSender<Request, Response>>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for LocalComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned,
    Response: Send + Serialize + DeserializeOwned,
{
    async fn send(&self, request: Request) -> ClientResult<Response> {
        let (res_tx, mut res_rx) = channel::<Response>(1);
        let request_and_res_tx = ComponentRequestAndResponseSender { request, tx: res_tx };
        self.tx.send(request_and_res_tx).await.expect("Outbound connection should be open.");
        Ok(res_rx.recv().await.expect("Inbound connection should be open."))
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require transactions to be cloneable.
impl<Request, Response> Clone for LocalComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}
