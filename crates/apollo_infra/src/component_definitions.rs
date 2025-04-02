use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter, Result};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, info};
use validator::Validate;

use crate::component_client::ClientResult;

pub const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
const DEFAULT_CHANNEL_BUFFER_SIZE: usize = 32;
const DEFAULT_RETRIES: usize = 3;
const DEFAULT_IDLE_CONNECTIONS: usize = usize::MAX;
const DEFAULT_IDLE_TIMEOUT: u64 = 90;
const DEFAULT_RETRY_INTERVAL: u64 = 3;

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

#[derive(Debug, Error, Deserialize, Serialize, Clone)]
pub enum ServerError {
    #[error("Could not deserialize client request: {0}")]
    RequestDeserializationFailure(String),
}

// The communication configuration of the local component.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct LocalServerConfig {
    pub channel_buffer_size: usize,
}

impl SerializeConfig for LocalServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "channel_buffer_size",
            &self.channel_buffer_size,
            "The communication channel buffer size.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for LocalServerConfig {
    fn default() -> Self {
        Self { channel_buffer_size: DEFAULT_CHANNEL_BUFFER_SIZE }
    }
}

// TODO(Nadin): Move the RemoteClientConfig and RemoteServerConfig to relevant modules.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteClientConfig {
    pub retries: usize,
    pub idle_connections: usize,
    pub idle_timeout: u64,
    pub retry_interval: u64,
}

impl Default for RemoteClientConfig {
    fn default() -> Self {
        Self {
            retries: DEFAULT_RETRIES,
            idle_connections: DEFAULT_IDLE_CONNECTIONS,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            retry_interval: DEFAULT_RETRY_INTERVAL,
        }
    }
}

impl SerializeConfig for RemoteClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "retries",
                &self.retries,
                "The max number of retries for sending a message.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connections",
                &self.idle_connections,
                "The maximum number of idle connections to keep alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_timeout",
                &self.idle_timeout,
                "The duration in seconds to keep an idle connection open before closing.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retry_interval",
                &self.retry_interval,
                "The duration in seconds to wait between remote connection retries.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
