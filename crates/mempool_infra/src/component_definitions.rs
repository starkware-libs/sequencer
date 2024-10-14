use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{error, info};
use validator::Validate;

use crate::errors::ComponentError;

pub const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
const DEFAULT_CHANNEL_BUFFER_SIZE: usize = 32;
const DEFAULT_RETRIES: usize = 3;
const DEFAULT_IDLE_CONNECTIONS: usize = usize::MAX;
const DEFAULT_IDLE_TIMEOUT: u64 = 90;

#[async_trait]
pub trait ComponentRequestHandler<Request, Response> {
    async fn handle_request(&mut self, request: Request) -> Response;
}

#[async_trait]
pub trait ComponentStarter {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {} with the default starter.", type_name::<Self>());
        Ok(())
    }
}

pub struct ComponentCommunication<T: Send + Sync> {
    tx: Option<Sender<T>>,
    rx: Option<Receiver<T>>,
}

impl<T: Send + Sync> ComponentCommunication<T> {
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
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub request: Request,
    pub tx: Sender<Response>,
}

#[derive(Debug, Error, Deserialize, Serialize, Clone)]
pub enum ServerError {
    #[error("Could not deserialize client request: {0}")]
    RequestDeserializationFailure(String),
}

// TODO(Nadin): Refactor this into two separate structs: LocalClientConfig (empty struct for the
// client) and LocalServerConfig (which holds the current channel_buffer_size field).

// The communication configuration of the local component.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct LocalComponentCommunicationConfig {
    pub channel_buffer_size: usize,
}

impl SerializeConfig for LocalComponentCommunicationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "channel_buffer_size",
            &self.channel_buffer_size,
            "The communication channel buffer size.",
            ParamPrivacyInput::Public,
        )])
    }
}

impl Default for LocalComponentCommunicationConfig {
    fn default() -> Self {
        Self { channel_buffer_size: DEFAULT_CHANNEL_BUFFER_SIZE }
    }
}

// TODO(Nadin): Move the RemoteClientConfig and RemoteServerConfig to relevant modules.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteClientConfig {
    pub socket: SocketAddr,
    pub retries: usize,
    pub idle_connections: usize,
    pub idle_timeout: u64,
}

impl Default for RemoteClientConfig {
    fn default() -> Self {
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
        Self {
            socket,
            retries: DEFAULT_RETRIES,
            idle_connections: DEFAULT_IDLE_CONNECTIONS,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
}

impl SerializeConfig for RemoteClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "socket",
                &self.socket.to_string(),
                "The remote component socket.",
                ParamPrivacyInput::Public,
            ),
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
        ])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteServerConfig {
    pub socket: SocketAddr,
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);
        Self { socket }
    }
}

impl SerializeConfig for RemoteServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([ser_param(
            "socket",
            &self.socket.to_string(),
            "The remote component socket.",
            ParamPrivacyInput::Public,
        )])
    }
}
