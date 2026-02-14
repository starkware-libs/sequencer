use std::clone::Clone;
use std::sync::Arc;
use std::time::Instant;

use apollo_class_manager_types::SharedClassManagerClient;
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
use apollo_transaction_converter::{TransactionConverter, TransactionConverterTrait};
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
use tracing::{debug, error, info, warn};

use crate::errors::{
    mempool_client_result_to_deprecated_gw_result,
    transaction_converter_err_to_deprecated_gw_err,
    GatewayResult,
};
use crate::metrics::{register_metrics, GatewayMetricHandle, GATEWAY_ADD_TX_LATENCY};
use crate::proof_archive_writer::{
    GcsProofArchiveWriter,
    NoOpProofArchiveWriter,
    ProofArchiveWriterTrait,
};
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::{
    StatefulTransactionValidatorFactory,
    StatefulTransactionValidatorFactoryTrait,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator,
    StatelessTransactionValidatorTrait,
};
use crate::sync_state_reader::SyncStateReaderFactory;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

#[derive(Clone)]
pub struct Gateway(GenericGateway<StatelessTransactionValidator, TransactionConverter>);

impl Gateway {
    fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<SyncStateReaderFactory>,
        mempool_client: SharedMempoolClient,
        transaction_converter: Arc<TransactionConverter>,
        stateless_tx_validator: Arc<StatelessTransactionValidator>,
        proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
    ) -> Self {
        Self(GenericGateway::new(
            config,
            state_reader_factory,
            mempool_client,
            transaction_converter,
            stateless_tx_validator,
            proof_archive_writer,
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
pub(crate) struct GenericGateway<
    TStatelessValidator: StatelessTransactionValidatorTrait,
    TTransactionConverter: TransactionConverterTrait,
> {
    config: Arc<GatewayConfig>,
    stateless_tx_validator: Arc<TStatelessValidator>,
    stateful_tx_validator_factory: Arc<dyn StatefulTransactionValidatorFactoryTrait>,
    mempool_client: SharedMempoolClient,
    transaction_converter: Arc<TTransactionConverter>,
    proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
}

impl<
    TStatelessValidator: StatelessTransactionValidatorTrait,
    TTransactionConverter: TransactionConverterTrait,
> GenericGateway<TStatelessValidator, TTransactionConverter>
{
    pub(crate) fn new<StateReaderFactoryGeneric: StateReaderFactory + 'static>(
        config: GatewayConfig,
        state_reader_factory: Arc<StateReaderFactoryGeneric>,
        mempool_client: SharedMempoolClient,
        transaction_converter: Arc<TTransactionConverter>,
        stateless_tx_validator: Arc<TStatelessValidator>,
        proof_archive_writer: Arc<dyn ProofArchiveWriterTrait>,
    ) -> Self {
        Self {
            config: Arc::new(config.clone()),
            stateless_tx_validator,
            stateful_tx_validator_factory: Arc::new(StatefulTransactionValidatorFactory {
                config: config.stateful_tx_validator_config.clone(),
                chain_info: config.chain_info.clone(),
                state_reader_factory,
                contract_class_manager: ContractClassManager::start(
                    config.contract_class_manager_config.clone(),
                ),
            }),
            mempool_client,
            transaction_converter,
            proof_archive_writer,
        }
    }

    #[sequencer_latency_histogram(GATEWAY_ADD_TX_LATENCY, true)]
    pub(crate) async fn add_tx(
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

        let mut stateful_transaction_validator = self
            .stateful_tx_validator_factory
            .instantiate_validator()
            .await
            .inspect_err(|e| metric_counters.record_add_tx_failure(e))?;

        let nonce = stateful_transaction_validator
            .extract_state_nonce_and_run_validations(&executable_tx, self.mempool_client.clone())
            .await
            .inspect_err(|e| metric_counters.record_add_tx_failure(e))?;

        if let Some((proof_facts, proof)) = proof_data {
            let tx_hash = internal_tx.tx_hash;
            let proof_manager_client = self.transaction_converter.get_proof_manager_client();
            let proof_manager_store_start = Instant::now();
            // Proof is verified during conversion to internal tx. It is stored here, after
            // validation, to avoid storing proofs for rejected transactions.
            if let Err(e) = proof_manager_client.set_proof(proof_facts.clone(), proof.clone()).await
            {
                error!("Failed to set proof in proof manager: {}", e);
            }
            let proof_manager_store_duration = proof_manager_store_start.elapsed();
            info!(
                "Proof manager store took: {proof_manager_store_duration:?} for tx hash: \
                 {tx_hash:?}"
            );
            let proof_archive_writer_start = Instant::now();
            let proof_archive_writer = self.proof_archive_writer.clone();
            tokio::spawn(async move {
                if let Err(e) = proof_archive_writer.set_proof(proof_facts, proof).await {
                    error!("Failed to archive proof to GCS: {}", e);
                }
                let proof_archive_writer_duration = proof_archive_writer_start.elapsed();
                info!(
                    "Proof archive writer took: {proof_archive_writer_duration:?} for tx hash: \
                     {tx_hash:?}"
                );
            });
        }
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

        Ok(gateway_output)
    }

    fn check_declare_permissions(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> Result<(), StarknetError> {
        // TODO(noamsp): Return same error as in Python gateway.
        if self.config.block_declare {
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

        // Extract proof data from verification handle before awaiting.
        let proof_data = verification_handle
            .as_ref()
            .map(|handle| (handle.proof_facts.clone(), handle.proof.clone()));

        // Await the verification task immediately.
        if let Some(handle) = verification_handle {
            let task = handle.verification_task.lock().await.take();
            if let Some(task) = task {
                let verification_result = task.await.map_err(|e| {
                    warn!("Proof verification task join error: {}", e);
                    StarknetError::internal_with_logging("Proof verification task join error:", &e)
                })?;
                verification_result.map_err(|e| {
                    warn!("Proof verification failed: {}", e);
                    transaction_converter_err_to_deprecated_gw_err(tx_signature, e)
                })?;
            }
        }

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
}

pub fn create_gateway(
    config: GatewayConfig,
    shared_state_sync_client: SharedStateSyncClient,
    mempool_client: SharedMempoolClient,
    class_manager_client: SharedClassManagerClient,
    proof_manager_client: SharedProofManagerClient,
    runtime: tokio::runtime::Handle,
) -> Gateway {
    let state_reader_factory = Arc::new(SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client: class_manager_client.clone(),
        runtime,
    });
    let transaction_converter = Arc::new(TransactionConverter::new(
        class_manager_client,
        proof_manager_client,
        config.chain_info.chain_id.clone(),
    ));
    let stateless_tx_validator = Arc::new(StatelessTransactionValidator {
        config: config.stateless_tx_validator_config.clone(),
    });

    // Create proof archive writer: use NoOp if bucket name is empty, otherwise use real GCS.
    let proof_archive_writer: Arc<dyn ProofArchiveWriterTrait> =
        if config.proof_archive_writer_config.bucket_name.is_empty() {
            Arc::new(NoOpProofArchiveWriter)
        } else {
            Arc::new(GcsProofArchiveWriter::new(config.proof_archive_writer_config.clone()))
        };

    Gateway::new(
        config,
        state_reader_factory,
        mempool_client,
        transaction_converter,
        stateless_tx_validator,
        proof_archive_writer,
    )
}

#[async_trait]
impl ComponentStarter for Gateway {
    async fn start(&mut self) {
        register_metrics();
        self.0.proof_archive_writer.connect().await;
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
