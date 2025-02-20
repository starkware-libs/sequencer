#[cfg(test)]
mod cende_test;
mod central_objects;

use std::collections::BTreeMap;
use std::fs;
use std::future::ready;
use std::sync::Arc;

use async_trait::async_trait;
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use central_objects::{
    process_transactions,
    CentralBlockInfo,
    CentralBouncerWeights,
    CentralCasmContractClassEntry,
    CentralCompressedStateDiff,
    CentralSierraContractClassEntry,
    CentralStateDiff,
    CentralTransactionWritten,
};
#[cfg(test)]
use mockall::automock;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use reqwest::{Certificate, Client, ClientBuilder, RequestBuilder};
use serde::{Deserialize, Serialize};
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;
use starknet_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use tokio::sync::Mutex;
use tokio::task::{self, JoinHandle};
use tracing::debug;
use url::Url;

#[derive(thiserror::Error, Debug)]
pub enum CendeAmbassadorError {
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerClientError),
    #[error("Class of hash: {class_hash} not found")]
    ClassNotFound { class_hash: ClassHash },
    #[error(transparent)]
    StarknetApiError(#[from] starknet_api::StarknetApiError),
}

pub type CendeAmbassadorResult<T> = Result<T, CendeAmbassadorError>;

/// A chunk of all the data to write to Aersopike.
#[derive(Debug, Serialize)]
pub(crate) struct AerospikeBlob {
    // TODO(yael, dvir): add the Casm and Sierra contract class
    block_number: BlockNumber,
    state_diff: CentralStateDiff,
    // The batcher may return a `None` compressed state diff if it is disabled in the
    // configuration.
    compressed_state_diff: Option<CentralCompressedStateDiff>,
    bouncer_weights: CentralBouncerWeights,
    transactions: Vec<CentralTransactionWritten>,
    execution_infos: Vec<CentralTransactionExecutionInfo>,
    contract_classes: Vec<CentralSierraContractClassEntry>,
    compiled_classes: Vec<CentralCasmContractClassEntry>,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    /// `current_height` is the height of the block that is built when calling this function.
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool>;

    // Prepares the previous height blob that will be written in the next height.
    async fn prepare_blob_for_next_height(
        &self,
        blob_parameters: BlobParameters,
    ) -> CendeAmbassadorResult<()>;
}

#[derive(Clone)]
pub struct CendeAmbassador {
    // TODO(dvir): consider creating enum varaiant instead of the `Option<AerospikeBlob>`.
    // `None` indicates that there is no blob to write, and therefore, the node can't be the
    // proposer.
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
    url: Url,
    client: Client,
    skip_write_height: Option<BlockNumber>,
    class_manager: SharedClassManagerClient,
}

/// The path to write blob in the Recorder.
pub const RECORDER_WRITE_BLOB_PATH: &str = "/cende_recorder/write_blob";

impl CendeAmbassador {
    pub fn new(cende_config: CendeConfig, class_manager: SharedClassManagerClient) -> Self {
        CendeAmbassador {
            prev_height_blob: Arc::new(Mutex::new(None)),
            url: cende_config
                .recorder_url
                .join(RECORDER_WRITE_BLOB_PATH)
                .expect("Failed to join `RECORDER_WRITE_BLOB_PATH` with the Recorder URL"),
            client: Self::build_client(cende_config.certificates_file_path),
            skip_write_height: cende_config.skip_write_height,
            class_manager,
        }
    }

    fn build_client(certificates_file_path: Option<String>) -> Client {
        let mut client_builder = ClientBuilder::new();
        if let Some(certificates_file_path) = certificates_file_path {
            let content =
                fs::read(certificates_file_path).expect("Failed to read cende certificates file.");
            let certificates =
                Certificate::from_pem(&content).expect("Failed to parse the certificates");
            client_builder = client_builder.add_root_certificate(certificates);
        }
        client_builder.build().expect("Failed to build the client")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Url,
    pub skip_write_height: Option<BlockNumber>,
    pub certificates_file_path: Option<String>,
}

impl Default for CendeConfig {
    fn default() -> Self {
        CendeConfig {
            recorder_url: "https://recorder_url"
                .parse()
                .expect("recorder_url must be a valid Recorder URL"),
            skip_write_height: None,
            certificates_file_path: None,
        }
    }
}

impl SerializeConfig for CendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([ser_param(
            "recorder_url",
            &self.recorder_url,
            "The URL of the Pythonic cende_recorder",
            ParamPrivacyInput::Private,
        )]);
        config.extend(ser_optional_param(
            &self.skip_write_height,
            BlockNumber(0),
            "skip_write_height",
            "A height that the consensus can skip writing to Aerospike. Needed for booting up (no \
             previous height blob to write) or to handle extreme cases (all the nodes failed).",
            ParamPrivacyInput::Private,
        ));
        config.extend(ser_optional_param(
            &self.certificates_file_path,
            "".to_string(),
            "certificates_file_path",
            "The path to the certificates file. The certificates are used when sending a request \
             to the cende_recorder.",
            ParamPrivacyInput::Private,
        ));
        config
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool> {
        // TODO(dvir): consider returning a future that will be spawned in the context instead.
        if self.skip_write_height == Some(current_height) {
            debug!(
                "Height {current_height} is configured as the `skip_write_height`, meaning \
                 consensus can send a proposal without writing to Aerospike. The blob that should \
                 have been written here in a normal flow, should already be written to Aerospike. \
                 Not writing to Aerospike previous height blob!!!.",
            );
            return tokio::spawn(ready(true));
        }

        let prev_height_blob = self.prev_height_blob.clone();
        let request_builder = self.client.post(self.url.clone());

        task::spawn(async move {
            // TODO(dvir): consider extracting the "should write blob" logic to a function.
            let Some(ref blob): Option<AerospikeBlob> = *prev_height_blob.lock().await else {
                // This case happens when restarting the node, `prev_height_blob` intial value is
                // `None`.
                debug!("No blob to write to Aerospike.");
                return false;
            };

            // Can happen in case the consensus got a block from the state sync and due to that did
            // not update the cende ambassador in `decision_reached` function.
            // TODO(dvir): what to do in the case of the `blob.block_number.0 >= height.0`? this
            // means a bug.
            if blob.block_number.0 + 1 != current_height.0 {
                debug!(
                    "Mismatch blob block number and height, can't write blob to Aerospike. Blob \
                     block number {}, height {}",
                    blob.block_number, current_height
                );
                return false;
            }

            debug!("Writing blob to Aerospike.");
            return send_write_blob(request_builder, blob).await;
        })
    }

    async fn prepare_blob_for_next_height(
        &self,
        blob_parameters: BlobParameters,
    ) -> CendeAmbassadorResult<()> {
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        *self.prev_height_blob.lock().await = Some(
            AerospikeBlob::from_blob_parameters_and_class_manager(
                blob_parameters,
                self.class_manager.clone(),
            )
            .await?,
        );
        Ok(())
    }
}

async fn send_write_blob(request_builder: RequestBuilder, blob: &AerospikeBlob) -> bool {
    // TODO(dvir): consider set `prev_height_blob` to `None` after writing to AS.
    match request_builder.json(blob).send().await {
        Ok(response) => {
            if response.status().is_success() {
                debug!("Blob written to Aerospike successfully.");
                true
            } else {
                debug!(
                    "The recorder failed to write blob.\nStatus code: {}\nMessage: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
                false
            }
        }
        Err(err) => {
            // TODO(dvir): try to test this case.
            debug!("Failed to send a request to the recorder. Error: {}", err);
            false
        }
    }
}

#[derive(Debug, Default)]
pub struct BlobParameters {
    // TODO(dvir): add here all the information needed for creating the blob: classes,
    // bouncer_weights.
    pub(crate) block_info: BlockInfo,
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) compressed_state_diff: Option<CommitmentStateDiff>,
    pub(crate) bouncer_weights: BouncerWeights,
    pub(crate) transactions: Vec<InternalConsensusTransaction>,
    pub(crate) execution_infos: Vec<TransactionExecutionInfo>,
}

impl AerospikeBlob {
    async fn from_blob_parameters_and_class_manager(
        blob_parameters: BlobParameters,
        class_manager: SharedClassManagerClient,
    ) -> CendeAmbassadorResult<Self> {
        let block_number = blob_parameters.block_info.block_number;
        let block_timestamp = blob_parameters.block_info.block_timestamp.0;

        let block_info =
            CentralBlockInfo::from((blob_parameters.block_info, StarknetVersion::LATEST));
        let state_diff = CentralStateDiff::from((blob_parameters.state_diff, block_info.clone()));
        let compressed_state_diff =
            blob_parameters.compressed_state_diff.map(|compressed_state_diff| {
                CentralStateDiff::from((compressed_state_diff, block_info))
            });

        let (central_transactions, contract_classes, compiled_classes) =
            process_transactions(class_manager, blob_parameters.transactions, block_timestamp)
                .await?;

        let execution_infos = blob_parameters
            .execution_infos
            .into_iter()
            .map(CentralTransactionExecutionInfo::from)
            .collect();

        Ok(AerospikeBlob {
            block_number,
            state_diff,
            compressed_state_diff,
            bouncer_weights: blob_parameters.bouncer_weights,
            transactions: central_transactions,
            execution_infos,
            contract_classes,
            compiled_classes,
        })
    }
}
