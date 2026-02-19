#[cfg(test)]
mod cende_test;
mod central_objects;

use std::sync::Arc;

use apollo_class_manager_types::{ClassManagerClientError, SharedClassManagerClient};
use apollo_config::secrets::Sensitive;
use apollo_consensus::types::ProposalCommitment;
use apollo_consensus_orchestrator_config::config::CendeConfig;
use apollo_proc_macros::sequencer_latency_histogram;
use apollo_sizeof::SizeOf;
use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::CompiledClassHashesForMigration;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::execution::call_info::CallInfo;
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
use reqwest::{Request, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use serde::Serialize;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use starknet_api::block::{BlockInfo, BlockNumber, StarknetVersion};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::fields::Calldata;
use tokio::sync::Mutex;
use tokio::task::{self, JoinHandle};
use tracing::{error, info, warn, Instrument};
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
    pub execution_infos: Vec<CentralTransactionExecutionInfo>,
    contract_classes: Vec<CentralSierraContractClassEntry>,
    compiled_classes: Vec<CentralCasmContractClassEntry>,
    casm_hash_computation_data_sierra_gas: CentralCasmHashComputationData,
    casm_hash_computation_data_proving_gas: CentralCasmHashComputationData,
    compiled_class_hashes_for_migration: CentralCompiledClassHashesForMigration,
    proposal_commitment: ProposalCommitment,
    parent_proposal_commitment: Option<ProposalCommitment>,
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
    prev_height_blob: Arc<Mutex<Option<AerospikeBlob>>>,
    url: Sensitive<Url>,
    client: ClientWithMiddleware,
    class_manager: SharedClassManagerClient,
}

/// The path to write blob in the Recorder.
pub const RECORDER_WRITE_BLOB_PATH: &str = "/cende_recorder/write_blob";

impl CendeAmbassador {
    pub fn new(cende_config: CendeConfig, class_manager: SharedClassManagerClient) -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(cende_config.min_retry_interval_ms, cende_config.max_retry_interval_ms)
            .jitter(Jitter::None)
            .build_with_total_retry_duration(cende_config.max_retry_duration_secs);

        CendeAmbassador {
            prev_height_blob: Arc::new(Mutex::new(None)),
            url: {
                let mut recorder_url = cende_config.recorder_url;
                recorder_url.append_route(RECORDER_WRITE_BLOB_PATH);
                recorder_url
            },
            client: ClientBuilder::new(reqwest::Client::new())
                .with(RetryTransientMiddleware::new_with_policy(retry_policy))
                .build(),
            class_manager,
        }
    }
}

#[async_trait]
impl CendeContext for CendeAmbassador {
    fn write_prev_height_blob(&self, current_height: BlockNumber) -> JoinHandle<bool> {
        info!("Start writing to Aerospike previous height blob for height {current_height}.");

        let prev_height_blob = self.prev_height_blob.clone();
        let request_builder = self.client.post(self.url.clone().expose_secret()); // TODO(victork): make sure we're allowed to expose the URL here

        task::spawn(
            async move {
                // TODO(dvir): consider extracting the "should write blob" logic to a function.
                let Some(ref blob): Option<AerospikeBlob> = *prev_height_blob.lock().await else {
                    // This case happens when restarting the node, `prev_height_blob` initial value
                    // is `None`.
                    warn!("CENDE_FAILURE: No blob to write to Aerospike.");
                    record_write_failure(CendeWriteFailureReason::BlobNotAvailable);
                    return false;
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
                return send_write_blob(request_builder, blob).await;
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

impl SizeOf for AerospikeBlob {
    fn dynamic_size(&self) -> usize {
        self.transactions.len() * std::mem::size_of::<CentralTransactionWritten>()
            + self.execution_infos.len() * std::mem::size_of::<CentralTransactionExecutionInfo>()
            + self.contract_classes.len() * std::mem::size_of::<CentralSierraContractClassEntry>()
            + self.compiled_classes.len() * std::mem::size_of::<CentralCasmContractClassEntry>()
            + self.state_diff.dynamic_size()
            + self.compressed_state_diff.as_ref().map_or(0, |d| d.dynamic_size())
            + self
                .casm_hash_computation_data_sierra_gas
                .class_hash_to_casm_hash_computation_gas
                .len()
                * std::mem::size_of::<(ClassHash, GasAmount)>()
            + self
                .casm_hash_computation_data_proving_gas
                .class_hash_to_casm_hash_computation_gas
                .len()
                * std::mem::size_of::<(ClassHash, GasAmount)>()
            + self.compiled_class_hashes_for_migration.len()
                * std::mem::size_of::<(CompiledClassHash, CompiledClassHash)>()
    }
}

#[sequencer_latency_histogram(CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY, false)]
async fn send_write_blob(request_builder: RequestBuilder, blob: &AerospikeBlob) -> bool {
    let (client, request) = request_builder.json(blob).build_split();
    let request = match request {
        Ok(req) => req,
        Err(err) => {
            warn!("CENDE_FAILURE: Failed to build request. Error: {err}");
            record_write_failure(CendeWriteFailureReason::CommunicationError);
            return false;
        }
    };

    analyze_blob(&request, blob).await;

    // TODO(dvir): use compression to reduce the size of the blob in the network.
    match client.execute(request).await {
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

static BLOB_DUMPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[allow(clippy::too_many_arguments)]
fn check_inner_calls_recursive(
    outer_calldata: &Calldata,
    inner_calls: &[CallInfo],
    same_arc: &mut usize,
    size_in_range: &mut usize,
    inner_tail_matches: &mut usize,
    execute_tail_matches: &mut usize,
    total: &mut usize,
    max_depth: &mut usize,
    max_deviation_felts: &mut usize,
    current_depth: usize,
) {
    *max_depth = (*max_depth).max(current_depth);
    for inner in inner_calls {
        *total += 1;
        if Arc::ptr_eq(&outer_calldata.0, &inner.call.calldata.0) {
            *same_arc += 1;
        }
        let outer_len = outer_calldata.0.len();
        let inner_len = inner.call.calldata.0.len();
        let deviation = outer_len.abs_diff(inner_len);
        *max_deviation_felts = (*max_deviation_felts).max(deviation);
        let within_range = outer_len == 0
            || (inner_len as f64 - outer_len as f64).abs() / outer_len as f64 <= 0.05;
        if within_range {
            *size_in_range += 1;
            if inner_len > outer_len
                && inner.call.calldata.0[inner_len - outer_len..] == outer_calldata.0[..]
            {
                *inner_tail_matches += 1;
            } else if outer_len > inner_len
                && outer_calldata.0[outer_len - inner_len..] == inner.call.calldata.0[..]
            {
                *execute_tail_matches += 1;
            }
        }
        if !inner.inner_calls.is_empty() {
            check_inner_calls_recursive(
                outer_calldata,
                &inner.inner_calls,
                same_arc,
                size_in_range,
                inner_tail_matches,
                execute_tail_matches,
                total,
                max_depth,
                max_deviation_felts,
                current_depth + 1,
            );
        }
    }
}

fn check_call_data(blob: &AerospikeBlob) -> String {
    let mut same_count = 0;
    let mut total_count = 0;
    let mut inner_same_arc = 0;
    let mut inner_size_in_range = 0;
    let mut inner_tail_matches = 0;
    let mut execute_tail_matches = 0;
    let mut inner_total = 0;
    let mut max_depth = 0;
    let mut max_deviation_felts = 0;
    let mut tx_exec_same_arc = 0;
    let mut tx_exec_total = 0;

    for (tx_written, exec_info) in blob.transactions.iter().zip(blob.execution_infos.iter()) {
        if let (Some(validate), Some(execute)) =
            (&exec_info.validate_call_info, &exec_info.execute_call_info)
        {
            total_count += 1;
            if Arc::ptr_eq(&validate.call.calldata.0, &execute.call.calldata.0) {
                same_count += 1;
            }

            check_inner_calls_recursive(
                &execute.call.calldata,
                &execute.inner_calls,
                &mut inner_same_arc,
                &mut inner_size_in_range,
                &mut inner_tail_matches,
                &mut execute_tail_matches,
                &mut inner_total,
                &mut max_depth,
                &mut max_deviation_felts,
                1,
            );
        }

        if let (Some(tx_calldata), Some(execute)) =
            (tx_written.invoke_calldata(), &exec_info.execute_call_info)
        {
            tx_exec_total += 1;
            if Arc::ptr_eq(&tx_calldata.0, &execute.call.calldata.0) {
                tx_exec_same_arc += 1;
            }
        }
    }

    format!(
        "Calldata Arc analysis:\nvalidate/execute same Arc: {same_count}/{total_count} \
         txs\ntransaction calldata/execute calldata same Arc: {tx_exec_same_arc}/{tx_exec_total} \
         invoke txs\nexecute outer/any inner_call same Arc (recursive): \
         {inner_same_arc}/{inner_total} inner_calls\nexecute outer/any inner_call size within 5% \
         (recursive): {inner_size_in_range}/{inner_total} inner_calls\ninner calldata tail \
         matches execute calldata (inner bigger, in range): {inner_tail_matches} cases\nexecute \
         calldata tail matches inner calldata (execute bigger, in range): {execute_tail_matches} \
         cases\nmax inner_calls recursion depth: {max_depth}\nmax calldata size deviation from \
         execute outer: {max_deviation_felts} felts\n"
    )
}

async fn analyze_blob(request: &Request, blob: &AerospikeBlob) {
    let Some(body) = request.body().and_then(|b| b.as_bytes()) else {
        return;
    };

    info!(
        "Blob with {} transactions - in-memory size: {} bytes, serialized size: {} bytes.",
        blob.transactions.len(),
        blob.size_bytes(),
        body.len()
    );

    if blob.transactions.len() > 200
        && BLOB_DUMPED
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_ok()
    {
        let path = format!("/tmp/blob_{}.json", blob.block_number);
        if let Err(err) = tokio::fs::write(&path, body).await {
            warn!("Failed to save blob to {path}: {err}");
        }

        if let Ok(serde_json::Value::Object(map)) = serde_json::from_slice(body) {
            let mut sizes_content = check_call_data(blob);
            sizes_content.push('\n');
            for (key, value) in &map {
                let size = serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0);
                sizes_content.push_str(&format!("{key}: {size} bytes\n"));
            }

            if let Some(serde_json::Value::Array(infos)) = map.get("execution_infos") {
                let mut field_sizes: std::collections::HashMap<String, usize> = Default::default();
                for info in infos {
                    if let serde_json::Value::Object(info_map) = info {
                        for (key, value) in info_map {
                            let size = serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0);
                            *field_sizes.entry(key.clone()).or_insert(0) += size;
                        }
                    }
                }
                sizes_content.push_str("\nexecution_infos fields (total across all txs):\n");
                for (key, size) in &field_sizes {
                    sizes_content.push_str(&format!("  {key}: {size} bytes\n"));
                }

                let accumulate_subfield_sizes =
                    |field_name: &str| -> std::collections::HashMap<String, usize> {
                        let mut sizes: std::collections::HashMap<String, usize> =
                            Default::default();
                        for info in infos {
                            if let serde_json::Value::Object(info_map) = info {
                                if let Some(serde_json::Value::Object(call_info)) =
                                    info_map.get(field_name)
                                {
                                    for (key, value) in call_info {
                                        let size =
                                            serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0);
                                        *sizes.entry(key.clone()).or_insert(0) += size;
                                    }
                                }
                            }
                        }
                        sizes
                    };

                for field_name in ["validate_call_info", "execute_call_info"] {
                    let subfield_sizes = accumulate_subfield_sizes(field_name);
                    if !subfield_sizes.is_empty() {
                        sizes_content
                            .push_str(&format!("\n{field_name} fields (total across all txs):\n"));
                        for (key, size) in &subfield_sizes {
                            sizes_content.push_str(&format!("  {key}: {size} bytes\n"));
                        }
                    }
                }
            }

            let sizes_path = format!("/tmp/blob_{}_sizes.txt", blob.block_number);
            if let Err(err) = tokio::fs::write(&sizes_path, sizes_content).await {
                warn!("Failed to save blob sizes to {sizes_path}: {err}");
            }
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
    pub(crate) proposal_commitment: ProposalCommitment,
    pub(crate) parent_proposal_commitment: Option<ProposalCommitment>,
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
            proposal_commitment: blob_parameters.proposal_commitment,
            parent_proposal_commitment: blob_parameters.parent_proposal_commitment,
        })
    }
}
