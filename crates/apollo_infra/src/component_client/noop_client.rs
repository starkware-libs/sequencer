use std::marker::PhantomData;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::component_client::{ClientError, ClientResult};
use crate::component_definitions::ComponentClient;
use crate::requests::LabeledRequest;

/// The `NoopComponentClient` struct is a generic client that always returns a Noop error.
/// Useful when a component should not actually process requests.
pub struct NoopComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    _phantom: PhantomData<Mutex<(Request, Response)>>,
}

impl<Request, Response> NoopComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
}

#[async_trait]
impl<Request, Response> ComponentClient<Request, Response>
    for NoopComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned + LabeledRequest,
    Response: Send + Serialize + DeserializeOwned,
{
    async fn send(&self, _request: Request) -> ClientResult<Response> {
        Err(ClientError::Noop)
    }
}

impl<Request, Response> Default for NoopComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    fn default() -> Self {
        Self::new()
    }
}

// Can't derive because derive forces the generics to also be `Clone`, which we prefer not to do
// since it'll require transactions to be cloneable.
impl<Request, Response> Clone for NoopComponentClient<Request, Response>
where
    Request: Send,
    Response: Send,
{
    fn clone(&self) -> Self {
        Self { _phantom: std::marker::PhantomData }
    }
}
