use std::fmt::{Debug, Formatter, Result};

use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, info};

use crate::component_client::ClientResult;

pub(crate) const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
pub const BUSY_PREVIOUS_REQUESTS_MSG: &str = "Server is busy addressing previous requests";

#[async_trait]
pub trait ComponentRequestHandler<Request, Response> {
    async fn handle_request(&mut self, request: Request) -> Response;
}

#[async_trait]
pub trait ComponentClient<Request, Response>
where
    Request: Send + Serialize + DeserializeOwned,
    Response: Send + Serialize + DeserializeOwned,
{
    async fn send(&self, request: Request) -> ClientResult<Response>;
}

pub async fn default_component_start_fn<T: ComponentStarter + ?Sized>() {
    info!("Starting component {} with the default starter.", short_type_name::<T>());
}

// Generic std::fmt::debug implementation for request and response enums. Requires the
// request/response to support returning a string representation of the enum, e.g., by deriving
// `strum_macros::AsRefStr`.
pub fn default_request_response_debug_impl<T: AsRef<str>>(
    f: &mut Formatter<'_>,
    value: &T,
) -> Result {
    write!(f, "{}::{}", short_type_name::<T>(), value.as_ref())
}

#[macro_export]
macro_rules! impl_debug_for_infra_requests_and_responses {
    ($t:ty) => {
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $crate::component_definitions::default_request_response_debug_impl(f, self)
            }
        }
    };
}

#[async_trait]
pub trait ComponentStarter {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await
    }
}

pub struct ComponentCommunication<T: Send> {
    tx: Option<Sender<T>>,
    rx: Option<Receiver<T>>,
}

impl<T: Send> ComponentCommunication<T> {
    pub fn new(tx: Option<Sender<T>>, rx: Option<Receiver<T>>) -> Self {
        Self { tx, rx }
    }

    pub fn take_tx(&mut self) -> Sender<T> {
        self.tx.take().expect("Sender should be available, could be taken only once")
    }

    pub fn take_rx(&mut self) -> Receiver<T> {
        self.rx.take().expect("Receiver should be available, could be taken only once")
    }
}

pub struct ComponentRequestAndResponseSender<Request, Response>
where
    Request: Send,
    Response: Send,
{
    pub request: Request,
    pub tx: Sender<Response>,
}

#[derive(Debug, Error, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum ServerError {
    #[error("Could not deserialize client request: {0}")]
    RequestDeserializationFailure(String),
}
