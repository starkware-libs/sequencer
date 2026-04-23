#[cfg(test)]
mod cende_test;
mod central_objects;

use std::sync::Arc;

use apollo_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use apollo_consensus::types::ProposalCommitment;
use apollo_consensus_orchestrator_config::config::CendeConfig;
use apollo_proc_macros::sequencer_latency_histogram;
use async_trait::async_trait;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
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
use starknet_api::block::{BlockHashAndNumber, BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::ClassHash;
use starknet_api::state::ThinStateDiff;
use tokio::sync::Mutex;
use tokio::task::{self, JoinHandle};
use tracing::{info, warn, Instrument};
use url::Url;

use crate::fee_market::FeeMarketInfo;
use crate::metrics::{
    record_write_failure,
    CendeWriteFailureReason,
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

/// Number of recent block hashes to include in the blob.
pub(crate) const N_BLOCK_HASHES_BACK_IN_BLOB: u64 = STORED_BLOCK_HASH_BUFFER;

pub type CendeAmbassadorResult<T> = Result<T, CendeAmbassadorError>;

/// A chunk of all the data to write to Aersopike.
#[derive(Debug, Serialize)]
pub struct AerospikeBlob {
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
    proposal_commitment: ProposalCommitment,
    parent_proposal_commitment: Option<ProposalCommitment>,
    recent_block_hashes: Vec<BlockHashAndNumber>,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait CendeContext: Send + Sync {
    /// Write the previous height blob to Aerospike. Returns a cell with an inner boolean indicating
    /// whether the write was successful.
    /// `current_height` is the height of the block that is built when calling this function.
    /// This function should return false if the previous height blob is not available.
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
    prev_height_blob: Arc<Mutex<Option<Arc<AerospikeBlob>>>>,
    write_blob_url: Url,
    get_latest_received_block_url: Url,
    client: ClientWithMiddleware,
    class_manager: SharedClassManagerClient,
}

/// The path to write blob in the Recorder.
pub const RECORDER_WRITE_BLOB_PATH: &str = "/cende_recorder/write_blob";
/// The path to get the latest received block from the Recorder (the next block that will be written
/// to DB. returns null when no blocks exist).
pub const RECORDER_GET_LATEST_RECEIVED_BLOCK_PATH: &str =
    "/cende_recorder/get_latest_received_block";

#[derive(Debug, Deserialize)]
struct GetLatestReceivedBlockResponse {
    block_number: Option<u64>,
}

impl CendeAmbassador {
    pub fn new(cende_config: CendeConfig, class_manager: SharedClassManagerClient) -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(cende_config.min_retry_interval_ms, cende_config.max_retry_interval_ms)
            .jitter(Jitter::None)
            .build_with_total_retry_duration(cende_config.max_retry_duration_secs);

        CendeAmbassador {
            prev_height_blob: Arc::new(Mutex::new(None)),
            write_blob_url: cende_config
                .recorder_url
                .join(RECORDER_WRITE_BLOB_PATH)
                .expect("Failed to construct write blob URL"),
            get_latest_received_block_url: cende_config
                .recorder_url
                .join(RECORDER_GET_LATEST_RECEIVED_BLOCK_PATH)
                .expect("Failed to construct get latest received block URL"),
            client: ClientBuilder::new(reqwest::Client::new())
                .with(RetryTransientMiddleware::new_with_policy(retry_policy))
                .build(),
            class_manager,
        }
    }
}

/// Returns whether the previous block exists at cende for  the current height.
async fn previous_height_exists_at_cende_recorder(
    client: ClientWithMiddleware,
    url: Url,
    current_height: BlockNumber,
) -> bool {
    // No previous block needed for height 0.
    if current_height == BlockNumber(0) {
        info!("Block 0 has no previous block. Proceeding.");
        return true;
    }

    let latest_received_block = fetch_latest_received_block(&client, &url).await;

    // No latest received block, so no previous block.
    let Some(latest) = latest_received_block else {
        warn!("CENDE_FAILURE: Cende does not have previous block for height {current_height}.");
        record_write_failure(CendeWriteFailureReason::NoLatestBlockFromRecorder);
        return false;
    };

    // Cende has a block at or above the current height, fail the round.
    if latest >= current_height {
        warn!(
            "Cende ahead of proposal height {current_height} (cende at {latest}). Cannot proceed \
             with this round."
        );
        record_write_failure(CendeWriteFailureReason::RecorderAheadOfProposalHeight);
        return false;
    }

    // Has previous block.
    let prev = current_height.prev().unwrap();
    if latest >= prev {
        info!("Cende already has previous block for height {current_height}. Skipping write.");
        return true;
    }

    // Highly unlikely: Cende behind previous block. Warn for debugging, no metric.
    warn!(
        "CENDE_FAILURE: Cende behind prev for height {current_height} (cende latest={latest}). \
         Cannot proceed."
    );
    false
}

async fn fetch_latest_received_block(
    client: &ClientWithMiddleware,
    url: &Url,
) -> Option<BlockNumber> {
    match client.get(url.as_str()).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<GetLatestReceivedBlockResponse>().await {
                Ok(resp) => resp.block_number.map(BlockNumber),
                Err(e) => {
                    warn!("Failed to parse recorder get_latest_received_block response: {e}");
                    None
                }
            }
        }
        Ok(response) => {
            warn!(
                "Recorder get_latest_received_block returned error status {}: {}",
                response.status(),
                response.text().await.unwrap_or_else(|_| "unparseable".to_string())
            );
            None
        }
        Err(e) => {
            warn!("Failed to request recorder get_latest_received_block: {e}");
            None
        }
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool> {
        info!("Start writing to Aerospike previous height blob for height {current_height}.");

        let prev_height_blob = self.prev_height_blob.clone();
        let request_builder = self.client.post(self.write_blob_url.clone());
        let client = self.client.clone();
        let get_latest_url = self.get_latest_received_block_url.clone();

        task::spawn(
            async move {
                let prev_blob: Option<Arc<AerospikeBlob>> = {
                    let guard = prev_height_blob.lock().await;
                    (*guard).clone()
                };

                let Some(blob) = prev_blob else {
                    return previous_height_exists_at_cende_recorder(
                        client,
                        get_latest_url,
                        current_height,
                    )
                    .await;
                };

                if blob.block_number.0 >= current_height.0 {
                    panic!(
                        "Blob block number is greater than or equal to the current height. That \
                         means cende has a blob of height that hasn't reached a consensus."
                    )
                }

                // Can happen in case the consensus got a block from the state sync and due to that
                // did not update the cende ambassador in `decision_reached` function.
                if blob.block_number.0 + 1 != current_height.0 {
                    warn!(
                        "CENDE_FAILURE: Mismatch blob block number and height, can't write blob \
                         to Aerospike. Blob block number {}, height {current_height}",
                        blob.block_number
                    );
                    record_write_failure(CendeWriteFailureReason::HeightMismatch);
                    return false;
                }

                info!("Writing blob to Aerospike.");
                return send_write_blob(request_builder, blob.as_ref()).await;
            }
            .instrument(tracing::debug_span!("cende write_prev_height_blob height")),
        )
    }

    #[sequencer_latency_histogram(CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY, false)]
    async fn prepare_blob_for_next_height(
        &self,
        blob_parameters: BlobParameters,
    ) -> CendeAmbassadorResult<()> {
        // TODO(dvir): as optimization, call the `into` and other preperation when writing to AS.
        let block_number = blob_parameters.block_info.block_number;
        *self.prev_height_blob.lock().await = Some(Arc::new(
            AerospikeBlob::from_blob_parameters_and_class_manager(
                blob_parameters,
                self.class_manager.clone(),
            )
            .await?,
        ));
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
                record_write_failure(CendeWriteFailureReason::CendeRecorderError);
                false
            }
        }
        Err(err) => {
            // TODO(dvir): try to test this case.
            warn!("CENDE_FAILURE: Failed to send a request to the recorder. Error: {err}");
            record_write_failure(CendeWriteFailureReason::CommunicationError);
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

#[derive(Debug)]
pub struct InternalTransactionWithReceipt {
    pub transaction: InternalConsensusTransaction,
    pub execution_info: TransactionExecutionInfo,
}

#[derive(Debug, Default)]
pub struct BlobParameters {
    pub block_info: BlockInfo,
    pub state_diff: ThinStateDiff,
    pub compressed_state_diff: Option<CommitmentStateDiff>,
    pub bouncer_weights: BouncerWeights,
    pub fee_market_info: FeeMarketInfo,
    pub transactions_with_execution_infos: Vec<InternalTransactionWithReceipt>,
    pub casm_hash_computation_data_sierra_gas: CasmHashComputationData,
    pub casm_hash_computation_data_proving_gas: CasmHashComputationData,
    // TODO(dvir): consider passing the execution_infos from the batcher as a string that
    // serialized in the correct format from the batcher.
    pub compiled_class_hashes_for_migration: CompiledClassHashesForMigration,
    pub proposal_commitment: ProposalCommitment,
    pub parent_proposal_commitment: Option<ProposalCommitment>,
    pub recent_block_hashes: Vec<BlockHashAndNumber>,
}

impl AerospikeBlob {
    pub async fn from_blob_parameters_and_class_manager(
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

        let (blob_transactions, blob_exec_infos): (
            Vec<InternalConsensusTransaction>,
            Vec<TransactionExecutionInfo>,
        ) = blob_parameters
            .transactions_with_execution_infos
            .into_iter()
            .map(|tx_with_exec_info| {
                (tx_with_exec_info.transaction, tx_with_exec_info.execution_info)
            })
            .unzip();

        let (central_transactions, contract_classes, compiled_classes) =
            process_transactions(class_manager, blob_transactions, block_timestamp).await?;

        let execution_infos =
            blob_exec_infos.into_iter().map(CentralTransactionExecutionInfo::from).collect();

        Ok(AerospikeBlob {
            block_number,
            state_diff,
            compressed_state_diff,
            bouncer_weights: blob_parameters.bouncer_weights.into(),
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
            proposal_commitment: blob_parameters.proposal_commitment,
            parent_proposal_commitment: blob_parameters.parent_proposal_commitment,
            recent_block_hashes: blob_parameters.recent_block_hashes,
        })
    }
}
