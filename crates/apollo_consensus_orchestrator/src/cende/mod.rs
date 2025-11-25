#[cfg(test)]
mod cende_test;
mod central_objects;

use std::sync::Arc;

use apollo_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use apollo_consensus_orchestrator_config::config::CendeConfig;
use apollo_proc_macros::sequencer_latency_histogram;
use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::CompiledClassHashesForMigration;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use central_objects::{
    process_transactions,
    CentralBlockInfo,
    CentralBouncerWeights,
    CentralCasmContractClassEntry,
    CentralCasmHashComputationData,
    CentralCompiledClassHashesForMigration,
    CentralCompressedStateDiff,
    CentralFeeMarketInfo,
    CentralSierraContractClassEntry,
    CentralStateDiff,
    CentralTransactionWritten,
};
#[cfg(test)]
use mockall::automock;
use reqwest::Response;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use serde::{Deserialize, Serialize};
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;
use tokio::sync::Mutex;
use tokio::task::{self, JoinHandle};
use tracing::{error, info, warn, Instrument};
use url::Url;

use crate::fee_market::FeeMarketInfo;
use crate::metrics::{
    record_write_failure,
    CendeWritePrevHeightFailureReason,
    CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    CENDE_WRITE_BLOB_SUCCESS,
    CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
};

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
    block_number: BlockNumber,
    state_diff: CentralStateDiff,
    // The batcher may return a `None` compressed state diff if it is disabled in the
    // configuration.
    compressed_state_diff: Option<CentralCompressedStateDiff>,
    bouncer_weights: CentralBouncerWeights,
    fee_market_info: CentralFeeMarketInfo,
    transactions: Vec<CentralTransactionWritten>,
    execution_infos: Vec<CentralTransactionExecutionInfo>,
    contract_classes: Vec<CentralSierraContractClassEntry>,
    compiled_classes: Vec<CentralCasmContractClassEntry>,
    casm_hash_computation_data_sierra_gas: CentralCasmHashComputationData,
    casm_hash_computation_data_proving_gas: CentralCasmHashComputationData,
    compiled_class_hashes_for_migration: CentralCompiledClassHashesForMigration,
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
    get_latest_blob_url: Url,
    write_blob_url: Url,
    client: ClientWithMiddleware,
    class_manager: SharedClassManagerClient,
}

/// The path to write blob in the Recorder.
pub const RECORDER_WRITE_BLOB_PATH: &str = "/cende_recorder/write_blob";

pub const RECORDER_GET_LATEST_BLOB_PATH: &str = "/cende_recorder/get_latest_blob";

#[derive(Debug, Deserialize, Serialize)]
struct GetLatestBlobResponse {
    height: BlockNumber,
    #[allow(dead_code)]
    // Curerntly unused but sent as part of the response from Cende.
    proposal_commitment: String,
}

impl CendeAmbassador {
    pub fn new(cende_config: CendeConfig, class_manager: SharedClassManagerClient) -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(cende_config.min_retry_interval_ms, cende_config.max_retry_interval_ms)
            .jitter(Jitter::None)
            .build_with_total_retry_duration(cende_config.max_retry_duration_secs);
        CendeAmbassador {
            prev_height_blob: Arc::new(Mutex::new(None)),
            get_latest_blob_url: cende_config
                .recorder_url
                .join(RECORDER_GET_LATEST_BLOB_PATH)
                .expect("Failed to join `RECORDER_GET_LATEST_BLOB_PATH` with the Recorder URL"),
            write_blob_url: cende_config
                .recorder_url
                .join(RECORDER_WRITE_BLOB_PATH)
                .expect("Failed to join `RECORDER_WRITE_BLOB_PATH` with the Recorder URL"),
            client: ClientBuilder::new(reqwest::Client::new())
                .with(RetryTransientMiddleware::new_with_policy(retry_policy))
                .build(),
            class_manager,
        }
    }
}

async fn is_block_available_in_cende(
    height: BlockNumber,
    get_latest_blob_request_builder: RequestBuilder,
) -> bool {
    match get_latest_blob_request_builder.send().await {
        Err(err) => {
            warn!("CENDE_FAILURE: Failed to get latest blob from the recorder. Error: {err}");
            record_write_failure(CendeWritePrevHeightFailureReason::CommunicationError);
            false
        }

        Ok(response) => {
            if !response.status().is_success() {
                warn!(
                    "CENDE_FAILURE: CENDE_FAILURE: Failed to get latest blob from the recorder. \
                     Status code: {}.  Response: {}",
                    response.status(),
                    response.text().await.unwrap_or("Unparsable response".to_owned()),
                );
                record_write_failure(CendeWritePrevHeightFailureReason::CendeRecorderError);
                return false;
            }

            let GetLatestBlobResponse { height: latest_blob_height, proposal_commitment: _ } =
                response.json().await.unwrap();
            // TODO(guy.f): Pass through the current block's commitment and compare it to the latest
            // blob's commitment.
            if latest_blob_height >= height {
                info!("Previous height blob is already available in Cende, No need to write it.");
                record_write_failure(CendeWritePrevHeightFailureReason::SkipWriteHeight);
                true
            } else {
                warn!(
                    "CENDE_FAILURE: Latest blob in cende is below the previous height. Cannot \
                     continue. Previous height: {height}, Latest blob height: {latest_blob_height}"
                );
                record_write_failure(CendeWritePrevHeightFailureReason::BlobNotAvailable);
                false
            }
        }
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool> {
        info!("Start writing to Aerospike previous height blob for height {current_height}.");

        let prev_height_blob = self.prev_height_blob.clone();
        let write_blob_request_builder = self.client.post(self.write_blob_url.clone());
        let get_latest_blob_request_builder = self.client.get(self.get_latest_blob_url.clone());

        let handle = task::spawn(
            async move {
                let Some(ref prev_blob): Option<AerospikeBlob> = *prev_height_blob.lock().await
                else {
                    // No previous blob stored.

                    let prev_height = current_height.prev();
                    if prev_height.is_none() {
                        info!(
                            "No previous blob, but current height is 0 so this is expected. \
                             Skipping write."
                        );
                        record_write_failure(CendeWritePrevHeightFailureReason::SkipWriteHeight);
                        return true;
                    }

                    info!(
                        "No previous blob, checking if the latest blob is available in Aerospike."
                    );
                    return is_block_available_in_cende(
                        prev_height.expect("Previous height should be Some. Already checked above"),
                        get_latest_blob_request_builder,
                    )
                    .await;
                };

                if prev_blob.block_number.0 >= current_height.0 {
                    panic!(
                        "Blob block number is greater than or equal to the current height. That \
                         means cende has a blob of height that hasn't reached a consensus."
                    )
                }

                // Can happen in case the consensus got a block from the state sync and due to that
                // did not update the cende ambassador in `decision_reached` function.
                if prev_blob.block_number.0 + 1 != current_height.0 {
                    warn!(
                        "CENDE_FAILURE: Mismatch blob block number and height, can't write blob \
                         to Aerospike. Blob block number {}, height {current_height}",
                        prev_blob.block_number
                    );
                    record_write_failure(CendeWritePrevHeightFailureReason::HeightMismatch);
                    return false;
                }

                info!("Writing blob to Aerospike.");
                return send_write_blob(write_blob_request_builder, prev_blob).await;
            }
            .instrument(tracing::debug_span!("cende write_prev_height_blob height")),
        );

        handle
    }

    #[sequencer_latency_histogram(CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY, false)]
    async fn prepare_blob_for_next_height(
        &self,
        blob_parameters: BlobParameters,
    ) -> CendeAmbassadorResult<()> {
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        let block_number = blob_parameters.block_info.block_number;
        *self.prev_height_blob.lock().await = Some(
            AerospikeBlob::from_blob_parameters_and_class_manager(
                blob_parameters,
                self.class_manager.clone(),
            )
            .await?,
        );
        info!("Blob for block number {block_number} is ready.");
        CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER.set_lossy(block_number.0);
        Ok(())
    }
}

#[sequencer_latency_histogram(CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY, false)]
async fn send_write_blob(request_builder: RequestBuilder, blob: &AerospikeBlob) -> bool {
    // TODO(dvir): use compression to reduce the size of the blob in the network.
    match request_builder.json(blob).send().await {
        Ok(response) => {
            if response.status().is_success() {
                info!(
                    "Blob with block number {} and {} transactions was written to Aerospike \
                     successfully.",
                    blob.block_number,
                    blob.transactions.len(),
                );
                print_write_blob_response(response).await;
                CENDE_WRITE_BLOB_SUCCESS.increment(1);
                true
            } else {
                warn!(
                    "CENDE_FAILURE: The recorder failed to write blob with block number {}. \
                     Status code: {}. Response: {}",
                    blob.block_number,
                    response.status(),
                    response.text().await.unwrap_or("Unparsable response".to_owned()),
                );
                record_write_failure(CendeWritePrevHeightFailureReason::CendeRecorderError);
                false
            }
        }
        Err(err) => {
            // TODO(dvir): try to test this case.
            warn!("CENDE_FAILURE: Failed to send a request to the recorder. Error: {err}");
            record_write_failure(CendeWritePrevHeightFailureReason::CommunicationError);
            false
        }
    }
}

async fn print_write_blob_response(response: Response) {
    info!("write blob response status code: {}", response.status());
    if let Ok(text) = response.text().await {
        info!("write blob response text: {text}");
    } else {
        info!("Failed to get response text.");
    }
}

#[derive(Debug, Default)]
pub struct BlobParameters {
    pub(crate) block_info: BlockInfo,
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) compressed_state_diff: Option<CommitmentStateDiff>,
    pub(crate) bouncer_weights: BouncerWeights,
    pub(crate) fee_market_info: FeeMarketInfo,
    pub(crate) transactions: Vec<InternalConsensusTransaction>,
    pub(crate) casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub(crate) casm_hash_computation_data_proving_gas: CasmHashComputationData,
    // TODO(dvir): consider passing the execution_infos from the batcher as a string that
    // serialized in the correct format from the batcher.
    pub(crate) execution_infos: Vec<TransactionExecutionInfo>,
    pub(crate) compiled_class_hashes_for_migration: CompiledClassHashesForMigration,
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
            fee_market_info: blob_parameters.fee_market_info,
            transactions: central_transactions,
            execution_infos,
            contract_classes,
            compiled_classes,
            casm_hash_computation_data_sierra_gas: blob_parameters
                .casm_hash_computation_data_sierra_gas,
            casm_hash_computation_data_proving_gas: blob_parameters
                .casm_hash_computation_data_proving_gas,
            compiled_class_hashes_for_migration: blob_parameters
                .compiled_class_hashes_for_migration,
        })
    }
}
