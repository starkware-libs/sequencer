use std::clone::Clone;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_config_manager_types::communication::{
    ConfigManagerReaderClient,
    LocalConfigManagerReaderClient,
};
use apollo_gateway_config::config::GatewayConfig;
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::gateway_types::{
    DeclareGatewayOutput,
    DeployAccountGatewayOutput,
    GatewayOutput,
    InvokeGatewayOutput,
};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_mempool_types::communication::{AddTransactionArgsWrapper, SharedMempoolClient};
use apollo_mempool_types::mempool_types::AddTransactionArgs;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_proc_macros::sequencer_latency_histogram;
use apollo_proof_manager_types::SharedProofManagerClient;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use apollo_transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
    VerificationHandle,
};
use async_trait::async_trait;
use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcTransaction,
};
use starknet_api::transaction::fields::{Proof, ProofFacts, TransactionSignature};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::errors::{
    mempool_client_result_to_deprecated_gw_result,
    transaction_converter_err_to_deprecated_gw_err,
    GatewayResult,
};
use crate::metrics::{
    register_metrics,
    GatewayMetricHandle,
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE,
    GATEWAY_PROOF_MANAGER_STORE_LATENCY,
};
use crate::proof_archive_writer::{
    GcsProofArchiveWriter,
    NoOpProofArchiveWriter,
    ProofArchiveError,
    ProofArchiveWriterTrait,
};
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::{
    StatefulTransactionValidatorFactory,
    StatefulTransactionValidatorFactoryTrait,
    StatefulTransactionValidatorTrait,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator,
    StatelessTransactionValidatorTrait,
};
use crate::sync_state_reader::SyncStateReaderFactory;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

const PROOF_ARCHIVE_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

type ProofArchiveHandle =
    Option<(JoinHandle<(Felt, Result<(), ProofArchiveError>)>, TransactionHash)>;

#[derive(Clone)]
pub struct Gateway(
    GenericGateway<
        StatelessTransactionValidator,
        TransactionConverter,
        StatefulTransactionValidatorFactory<SyncStateReaderFactory>,
    >,
);

impl Gateway {
    fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<SyncStateReaderFactory>,
        mempool_client: SharedMempoolClient,
        transaction_converter: Arc<TransactionConverter>,
        stateless_tx_validator: Arc<StatelessTransactionValidator>,
        proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
        config_manager_client: LocalConfigManagerReaderClient,
    ) -> Self {
        Self(GenericGateway::new(
            config,
            state_reader_factory,
            mempool_client,
            transaction_converter,
            stateless_tx_validator,
            proof_archive_writer,
            config_manager_client,
        ))
    }

    pub async fn add_tx(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        self.0.add_tx(tx, p2p_message_metadata).await
    }
}

#[derive(Clone)]
pub struct GenericGateway<
    TStatelessValidator: StatelessTransactionValidatorTrait,
    TTransactionConverter: TransactionConverterTrait,
    TStatefulValidatorFactory: StatefulTransactionValidatorFactoryTrait,
> {
    config: Arc<GatewayConfig>,
    stateless_tx_validator: Arc<TStatelessValidator>,
    stateful_tx_validator_factory: Arc<TStatefulValidatorFactory>,
    mempool_client: SharedMempoolClient,
    transaction_converter: Arc<TTransactionConverter>,
    proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
    config_manager_client: LocalConfigManagerReaderClient,
}

impl<
    TStatelessValidator: StatelessTransactionValidatorTrait,
    TTransactionConverter: TransactionConverterTrait,
    TStateReaderFactory: StateReaderFactory,
>
    GenericGateway<
        TStatelessValidator,
        TTransactionConverter,
        StatefulTransactionValidatorFactory<TStateReaderFactory>,
    >
{
    pub(crate) fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<TStateReaderFactory>,
        mempool_client: SharedMempoolClient,
        transaction_converter: Arc<TTransactionConverter>,
        stateless_tx_validator: Arc<TStatelessValidator>,
        proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
        config_manager_client: LocalConfigManagerReaderClient,
    ) -> Self {
        Self {
            config: Arc::new(config.clone()),
            stateless_tx_validator,
            stateful_tx_validator_factory: Arc::new(StatefulTransactionValidatorFactory {
                config: config.static_config.stateful_tx_validator_config.clone(),
                chain_info: config.static_config.chain_info.clone(),
                state_reader_factory,
                contract_class_manager: ContractClassManager::start(
                    config.static_config.contract_class_manager_config.clone(),
                ),
            }),
            mempool_client,
            transaction_converter,
            proof_archive_writer,
            config_manager_client,
        }
    }
}
impl<
    TStatelessValidator: StatelessTransactionValidatorTrait,
    TTransactionConverter: TransactionConverterTrait,
    TStatefulValidatorFactory: StatefulTransactionValidatorFactoryTrait,
> GenericGateway<TStatelessValidator, TTransactionConverter, TStatefulValidatorFactory>
{
    pub async fn start(&self) {
        register_metrics();
        self.proof_archive_writer.connect().await;
    }

    #[sequencer_latency_histogram(GATEWAY_ADD_TX_LATENCY, true)]
    pub async fn add_tx(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        debug!("Processing tx: {:?}", &tx);
        let tx_signature = tx.signature().clone();
        let is_p2p = p2p_message_metadata.is_some();

        let start_time = std::time::Instant::now();
        let ret = self.add_tx_inner(tx, p2p_message_metadata).await;
        let elapsed = start_time.elapsed().as_secs_f64();

        debug!(
            "Processed tx with signature: {:?}. duration: {elapsed} sec, ret: {ret:?}, is_p2p: \
             {is_p2p}",
            &tx_signature,
        );

        ret
    }

    async fn add_tx_inner(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        let mut metric_counters = GatewayMetricHandle::new(&tx, &p2p_message_metadata);
        metric_counters.count_transaction_received();

        if let RpcTransaction::Declare(ref declare_tx) = tx {
            if let Err(e) = self.check_declare_permissions(declare_tx) {
                metric_counters.record_add_tx_failure(&e);
                return Err(e);
            }
        }

        // Perform stateless validations.
        self.stateless_tx_validator.validate(&tx)?;

        let tx_signature = tx.signature().clone();
        let (internal_tx, executable_tx, proof_data) =
            self.convert_rpc_tx_to_internal_and_executable_txs(tx, &tx_signature).await?;

        let native_classes_whitelist = self
            .config_manager_client
            .get_gateway_dynamic_config()
            .expect("gateway dynamic config is not set")
            .native_classes_whitelist;
        let mut stateful_transaction_validator = self
            .stateful_tx_validator_factory
            .instantiate_validator(native_classes_whitelist)
            .await
            .inspect_err(|e| metric_counters.record_add_tx_failure(e))?;

        let nonce = stateful_transaction_validator
            .extract_state_nonce_and_run_validations(&executable_tx, self.mempool_client.clone())
            .await
            .inspect_err(|e| metric_counters.record_add_tx_failure(e))?;

        let proof_archive_handle =
            self.store_proof_and_spawn_archiving(proof_data, internal_tx.tx_hash).await;

        let gateway_output = create_gateway_output(&internal_tx);

        let add_tx_args = AddTransactionArgsWrapper {
            args: AddTransactionArgs::new(internal_tx, nonce),
            p2p_message_metadata,
        };
        let mempool_client_result = self.mempool_client.add_tx(add_tx_args).await;
        match mempool_client_result_to_deprecated_gw_result(&tx_signature, mempool_client_result) {
            Ok(()) => {}
            Err(e) => {
                metric_counters.record_add_tx_failure(&e);
                return Err(e);
            }
        };

        metric_counters.transaction_sent_to_mempool();

        // We await proof archiving only after the transaction is sent to the mempool to avoid
        // delays.
        Self::await_proof_archiving(proof_archive_handle).await;

        Ok(gateway_output)
    }

    async fn store_proof_and_spawn_archiving(
        &self,
        proof_data: Option<(ProofFacts, Proof)>,
        tx_hash: TransactionHash,
    ) -> ProofArchiveHandle {
        let (proof_facts, proof) = proof_data?;

        // Proof is verified during conversion to internal tx. It is stored here, after
        // validation, to avoid storing proofs for rejected transactions.
        let store_result = self
            .transaction_converter
            .store_proof_in_proof_manager(proof_facts.clone(), proof.clone())
            .await;
        match store_result {
            Ok(proof_manager_store_duration) => {
                GATEWAY_PROOF_MANAGER_STORE_LATENCY
                    .record(proof_manager_store_duration.as_secs_f64());
                info!(
                    "Proof manager store in the gateway took: {proof_manager_store_duration:?} \
                     for tx hash: {tx_hash:?}"
                );
            }
            Err(e) => {
                error!("Failed to set proof in proof manager: {}", e);
            }
        }

        let proof_archive_writer = self.proof_archive_writer.clone();
        let handle = tokio::spawn(async move {
            let proof_facts_hash = proof_facts.hash();
            let proof_archive_writer_start = Instant::now();
            let result = proof_archive_writer.set_proof(proof_facts, proof).await;
            let proof_archive_writer_duration = proof_archive_writer_start.elapsed();
            info!(
                "Proof archive writer took: {proof_archive_writer_duration:?} for tx hash: \
                 {tx_hash:?}"
            );
            (proof_facts_hash, result)
        });

        Some((handle, tx_hash))
    }

    async fn await_proof_archiving(proof_archive_handle: ProofArchiveHandle) {
        let Some((handle, tx_hash)) = proof_archive_handle else {
            return;
        };

        let abort_handle = handle.abort_handle();
        match timeout(PROOF_ARCHIVE_WRITE_TIMEOUT, handle).await {
            Ok(Ok((proof_facts_hash, Ok(())))) => {
                info!(
                    "Proof archived successfully. proof_facts_hash: {proof_facts_hash:?}, \
                     tx_hash: {tx_hash:?}"
                );
            }
            Ok(Ok((proof_facts_hash, Err(e)))) => {
                GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE.increment(1);
                error!(
                    "Failed to archive proof to GCS. proof_facts_hash: {proof_facts_hash:?}, \
                     tx_hash: {tx_hash:?}, error: {e}"
                );
            }
            Ok(Err(e)) => {
                GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE.increment(1);
                error!("Proof archive writer task panicked. tx_hash: {tx_hash:?}, error: {e}");
            }
            Err(_) => {
                abort_handle.abort();
                GATEWAY_PROOF_ARCHIVE_WRITE_FAILURE.increment(1);
                error!(
                    "Proof archive writer timed out after {PROOF_ARCHIVE_WRITE_TIMEOUT:?}. \
                     tx_hash: {tx_hash:?}"
                );
            }
        }
    }

    fn check_declare_permissions(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> Result<(), StarknetError> {
        // TODO(noamsp): Return same error as in Python gateway.
        if self.config.static_config.block_declare {
            return Err(StarknetError {
                code: StarknetErrorCode::UnknownErrorCode(
                    "StarknetErrorCode.BLOCKED_TRANSACTION_TYPE".to_string(),
                ),
                message: "Transaction type is temporarily blocked.".to_string(),
            });
        }
        let RpcDeclareTransaction::V3(declare_v3_tx) = declare_tx;
        if !self.config.is_authorized_declarer(&declare_v3_tx.sender_address) {
            return Err(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(
                    KnownStarknetErrorCode::UnauthorizedDeclare,
                ),
                message: format!(
                    "Account address {} is not allowed to declare contracts.",
                    &declare_v3_tx.sender_address
                ),
            });
        }
        Ok(())
    }

    async fn convert_rpc_tx_to_internal_and_executable_txs(
        &self,
        tx: RpcTransaction,
        tx_signature: &TransactionSignature,
    ) -> Result<
        (InternalRpcTransaction, AccountTransaction, Option<(ProofFacts, Proof)>),
        StarknetError,
    > {
        let (internal_tx, verification_handle) =
            self.transaction_converter.convert_rpc_tx_to_internal_rpc_tx(tx).await.map_err(
                |e| {
                    warn!("Failed to convert RPC transaction to internal RPC transaction: {}", e);
                    transaction_converter_err_to_deprecated_gw_err(tx_signature, e)
                },
            )?;

        // Await the verification task immediately.
        let proof_data = self
            .await_verification_task_and_extract_proof_data(verification_handle, tx_signature)
            .await?;

        let executable_tx = self
            .transaction_converter
            .convert_internal_rpc_tx_to_executable_tx(internal_tx.clone())
            .await
            .map_err(|e| {
                warn!("Failed to convert internal RPC transaction to executable transaction: {e}");
                transaction_converter_err_to_deprecated_gw_err(tx_signature, e)
            })?;

        Ok((internal_tx, executable_tx, proof_data))
    }
    async fn await_verification_task_and_extract_proof_data(
        &self,
        verification_handle: Option<VerificationHandle>,
        tx_signature: &TransactionSignature,
    ) -> Result<Option<(ProofFacts, Proof)>, StarknetError> {
        let Some(handle) = verification_handle else {
            return Ok(None);
        };

        handle
            .verification_task
            .await
            .map_err(|e| {
                warn!("Proof verification task panicked: {}", e);
                StarknetError::internal_with_logging("Proof verification task panicked:", &e)
            })?
            .map_err(|e| {
                warn!("Proof verification failed: {}", e);
                transaction_converter_err_to_deprecated_gw_err(tx_signature, e)
            })?;

        Ok(Some((handle.proof_facts, handle.proof)))
    }
}

pub fn create_gateway(
    config: GatewayConfig,
    shared_state_sync_client: SharedStateSyncClient,
    mempool_client: SharedMempoolClient,
    class_manager_client: SharedClassManagerClient,
    proof_manager_client: SharedProofManagerClient,
    runtime: tokio::runtime::Handle,
    config_manager_client: LocalConfigManagerReaderClient,
) -> Gateway {
    let state_reader_factory = Arc::new(SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client: class_manager_client.clone(),
        runtime,
    });
    let transaction_converter = Arc::new(TransactionConverter::new(
        class_manager_client,
        proof_manager_client,
        config.static_config.chain_info.chain_id.clone(),
    ));
    let stateless_tx_validator = Arc::new(StatelessTransactionValidator {
        config: config.static_config.stateless_tx_validator_config.clone(),
    });

    // Create proof archive writer: use NoOp if bucket name is empty, otherwise use real GCS.
    let proof_archive_writer: Arc<dyn ProofArchiveWriterTrait> =
        if config.static_config.proof_archive_writer_config.bucket_name.is_empty() {
            Arc::new(NoOpProofArchiveWriter)
        } else {
            Arc::new(GcsProofArchiveWriter::new(
                config.static_config.proof_archive_writer_config.clone(),
            ))
        };

    Gateway::new(
        config,
        state_reader_factory,
        mempool_client,
        transaction_converter,
        stateless_tx_validator,
        proof_archive_writer,
        config_manager_client,
    )
}

#[async_trait]
impl ComponentStarter for Gateway {
    async fn start(&mut self) {
        self.0.start().await;
    }
}

fn create_gateway_output(internal_rpc_tx: &InternalRpcTransaction) -> GatewayOutput {
    let transaction_hash = internal_rpc_tx.tx_hash;
    match &internal_rpc_tx.tx {
        InternalRpcTransactionWithoutTxHash::Declare(declare_tx) => GatewayOutput::Declare(
            DeclareGatewayOutput::new(transaction_hash, declare_tx.class_hash),
        ),
        InternalRpcTransactionWithoutTxHash::DeployAccount(deploy_account_tx) => {
            GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(
                transaction_hash,
                deploy_account_tx.contract_address,
            ))
        }
        InternalRpcTransactionWithoutTxHash::Invoke(_) => {
            GatewayOutput::Invoke(InvokeGatewayOutput::new(transaction_hash))
        }
    }
}
