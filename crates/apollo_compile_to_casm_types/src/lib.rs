use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    PrioritizedRequest,
};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::CompiledClassHash;
use starknet_api::state::SierraContractClass;
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;
pub type SierraCompilerClientResult<T> = Result<T, SierraCompilerClientError>;

pub type RawExecutableHashedClass = (RawExecutableClass, CompiledClassHash);

pub type LocalSierraCompilerClient =
    LocalComponentClient<SierraCompilerRequest, SierraCompilerResponse>;
pub type RemoteSierraCompilerClient =
    RemoteComponentClient<SierraCompilerRequest, SierraCompilerResponse>;
pub type SharedSierraCompilerClient = Arc<dyn SierraCompilerClient>;
pub type SierraCompilerRequestAndResponseSender =
    ComponentRequestAndResponseSender<SierraCompilerRequest, SierraCompilerResponse>;

// TODO(Elin): change to a more efficient serde (bytes, or something similar).
// A prerequisite for this is to solve serde-untagged lack of support.

type RawClassResult<T> = Result<T, RawClassError>;
pub type RawClass = SerializedClass<SierraContractClass>;
pub type RawExecutableClass = SerializedClass<ContractClass>;

#[derive(Debug, Error)]
pub enum RawClassError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    WriteError(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerializedClass<T>(serde_json::Value, std::marker::PhantomData<T>);

impl<T> SerializedClass<T> {
    pub fn into_value(self) -> serde_json::Value {
        self.0
    }

    pub fn size(&self) -> RawClassResult<usize> {
        Ok(serde_json::to_string_pretty(&self.0)?.len())
    }

    fn new(value: serde_json::Value) -> Self {
        Self(value, std::marker::PhantomData)
    }

    pub fn from_file(path: PathBuf) -> RawClassResult<Option<Self>> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        match serde_json::from_reader(BufReader::new(file)) {
            Ok(value) => Ok(Some(Self::new(value))),
            // In case the file was deleted/tempered with until actual read is done.
            Err(e) if e.is_io() && e.to_string().contains("No such file or directory") => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn write_to_file(self, path: PathBuf) -> RawClassResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open a file for writing, deleting any existing content.
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .expect("Failing to open file with given options is impossible");

        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.into_value())?;

        Ok(())
    }

    #[cfg(any(feature = "testing", test))]
    pub fn new_unchecked(value: serde_json::Value) -> Self {
        Self::new(value)
    }
}

impl TryFrom<SierraContractClass> for RawClass {
    type Error = serde_json::Error;

    fn try_from(class: SierraContractClass) -> Result<Self, Self::Error> {
        Ok(Self::new(serde_json::to_value(class)?))
    }
}

impl TryFrom<RawClass> for SierraContractClass {
    type Error = serde_json::Error;

    fn try_from(class: RawClass) -> Result<Self, Self::Error> {
        serde_json::from_value(class.0)
    }
}

impl TryFrom<ContractClass> for RawExecutableClass {
    type Error = serde_json::Error;

    fn try_from(class: ContractClass) -> Result<Self, Self::Error> {
        Ok(Self::new(serde_json::to_value(class)?))
    }
}

impl TryFrom<RawExecutableClass> for ContractClass {
    type Error = serde_json::Error;

    fn try_from(class: RawExecutableClass) -> Result<Self, Self::Error> {
        serde_json::from_value(class.0)
    }
}

/// Serves as the Sierra compilation unit's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait SierraCompilerClient: Send + Sync {
    async fn compile(
        &self,
        class: RawClass,
    ) -> SierraCompilerClientResult<RawExecutableHashedClass>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum SierraCompilerError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
}

#[derive(Clone, Debug, Error)]
pub enum SierraCompilerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    SierraCompilerError(#[from] SierraCompilerError),
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(SierraCompilerRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum SierraCompilerRequest {
    Compile(RawClass),
}
impl_debug_for_infra_requests_and_responses!(SierraCompilerRequest);
impl_labeled_request!(SierraCompilerRequest, SierraCompilerRequestLabelValue);
impl PrioritizedRequest for SierraCompilerRequest {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SierraCompilerResponse {
    Compile(SierraCompilerResult<RawExecutableHashedClass>),
}

#[async_trait]
impl<ComponentClientType> SierraCompilerClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<SierraCompilerRequest, SierraCompilerResponse>,
{
    async fn compile(
        &self,
        class: RawClass,
    ) -> SierraCompilerClientResult<RawExecutableHashedClass> {
        let request = SierraCompilerRequest::Compile(class);
        handle_all_response_variants!(
            SierraCompilerResponse,
            Compile,
            SierraCompilerClientError,
            SierraCompilerError,
            Direct
        )
    }
}
