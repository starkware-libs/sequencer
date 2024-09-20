use std::collections::BTreeMap;
use std::fmt::Debug;
use std::net::IpAddr;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender};
use validator::Validate;
use tracing::{debug, error, info, instrument};

const DEFAULT_CHANNEL_BUFFER_SIZE: usize = 32;
const DEFAULT_RETRIES: usize = 3;

#[cfg(test)]
#[path = "component_definitions_test.rs"]
pub mod component_definitions_test;

#[async_trait]
pub trait ComponentRequestHandler<Request, Response> {
    async fn handle_request(&mut self, request: Request) -> Response;
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

pub const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";

#[derive(Debug, Error, Deserialize, Serialize, Clone)]
pub enum ServerError {
    #[error("Could not deserialize client request: {0}")]
    RequestDeserializationFailure(String),
}

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

// The communication configuration of the remote component.
#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteComponentCommunicationConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub retries: usize,
}

impl SerializeConfig for RemoteComponentCommunicationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The remote component server ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "The remote component server port.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retries",
                &self.retries,
                "The max number of retries for sending a message.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for RemoteComponentCommunicationConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8080, retries: DEFAULT_RETRIES }
    }
}



// Generic wrapper struct
#[derive(Serialize, Deserialize, std::fmt::Debug)]
pub(crate) struct SerdeWrapper<T> {
    pub data: T,
}

// Trait to define our serialization and deserialization behavior
pub(crate) trait BincodeSerializable: Sized {
    fn to_bincode(&self) -> Result<Vec<u8>, bincode::Error>;
    fn from_bincode(bytes: &[u8]) -> Result<Self, bincode::Error>;
}

// Implement the trait for our wrapper
impl<T: Serialize + for<'de> Deserialize<'de>> BincodeSerializable for SerdeWrapper<T> where T: std::fmt::Debug {
    #[instrument]
    fn to_bincode(&self) -> Result<Vec<u8>, bincode::Error> {
        serialize(self)
    }

    #[instrument]
    fn from_bincode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        deserialize(bytes)
    }
}
