use std::clone::Clone;
use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
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
use apollo_state_sync_types::communication::SharedStateSyncClient;
use axum::async_trait;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcTransaction,
};
use tracing::{debug, info, warn, Span};

use crate::errors::{
    mempool_client_result_to_deprecated_gw_result,
    transaction_converter_err_to_deprecated_gw_err,
    GatewayResult,
};
use crate::metrics::{register_metrics, GatewayMetricHandle, GATEWAY_ADD_TX_LATENCY};
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
pub struct Gateway {
    pub config: Arc<GatewayConfig>,
    pub stateless_tx_validator: Arc<StatelessTransactionValidator>,
    pub stateful_tx_validator_factory: Arc<dyn StatefulTransactionValidatorFactoryTrait>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub mempool_client: SharedMempoolClient,
    pub transaction_converter: Arc<TransactionConverter>,
}

impl Gateway {
    pub fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<dyn StateReaderFactory>,
        mempool_client: SharedMempoolClient,
        transaction_converter: TransactionConverter,
    ) -> Self {
        Self {
            config: Arc::new(config.clone()),
            stateless_tx_validator: Arc::new(StatelessTransactionValidator {
                config: config.stateless_tx_validator_config.clone(),
            }),
            stateful_tx_validator_factory: Arc::new(StatefulTransactionValidatorFactory {
                config: config.stateful_tx_validator_config.clone(),
                chain_info: config.chain_info.clone(),
            }),
            state_reader_factory,
            mempool_client,
            transaction_converter: Arc::new(transaction_converter),
        }
    }

    #[sequencer_latency_histogram(GATEWAY_ADD_TX_LATENCY, true)]
    pub async fn add_tx(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        debug!("Processing tx with signature: {:?}", tx.signature());
        let is_p2p = p2p_message_metadata.is_some();

        let start_time = std::time::Instant::now();
        let ret = self.add_tx_inner(&tx, p2p_message_metadata).await;
        let elapsed = start_time.elapsed().as_secs_f64();

        debug!(
            "Processed tx with signature: {:?}. duration: {elapsed} sec, ret: {ret:?}, is_p2p: \
             {is_p2p}, tx: {:?}",
            tx.signature(),
            tx
        );

        ret
    }

    async fn add_tx_inner(
        &self,
        tx: &RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        let mut metric_counters = GatewayMetricHandle::new(tx, &p2p_message_metadata);
        metric_counters.count_transaction_received();

        if let RpcTransaction::Declare(ref declare_tx) = tx {
            if let Err(e) = self.check_declare_permissions(declare_tx) {
                metric_counters.record_add_tx_failure(&e);
                return Err(e);
            }
        }

        let tx_signature = tx.signature().clone();
        let internal_tx = self
            .transaction_converter
            .convert_rpc_tx_to_internal_rpc_tx(tx.clone())
            .await
            .map_err(|e| {
                warn!("Failed to convert RPC transaction to internal RPC transaction: {}", e);
                transaction_converter_err_to_deprecated_gw_err(&tx_signature, e)
            })?;

        let executable_tx = self
            .transaction_converter
            .convert_internal_rpc_tx_to_executable_tx(internal_tx.clone())
            .await
            .map_err(|e| {
                warn!(
                    "Failed to convert internal RPC transaction to executable transaction: {}",
                    e
                );
                transaction_converter_err_to_deprecated_gw_err(&tx_signature, e)
            })?;

        let blocking_task = ProcessTxBlockingTask::new(
            self,
            tx.clone(),
            internal_tx,
            executable_tx,
            tokio::runtime::Handle::current(),
        );
        // Run the blocking task in the current span.
        let curr_span = Span::current();
        let handle =
            tokio::task::spawn_blocking(move || curr_span.in_scope(|| blocking_task.process_tx()));
        let handle_result = handle.await;
        let add_tx_args = match handle_result {
            Ok(Ok(add_tx_args)) => add_tx_args,
            Ok(Err(starknet_err)) => {
                info!(
                    "Gateway validation failed for tx with signature: {:?} with error: {}",
                    tx.signature(),
                    starknet_err
                );
                metric_counters.record_add_tx_failure(&starknet_err);
                return Err(starknet_err);
            }
            Err(join_err) => {
                let err = StarknetError::internal_with_signature_logging(
                    "Failed to process tx",
                    tx.signature(),
                    join_err,
                );
                metric_counters.record_add_tx_failure(&err);
                return Err(err);
            }
        };

        let gateway_output = create_gateway_output(&add_tx_args.tx);

        let add_tx_args = AddTransactionArgsWrapper { args: add_tx_args, p2p_message_metadata };
        match mempool_client_result_to_deprecated_gw_result(
            tx.signature(),
            self.mempool_client.add_tx(add_tx_args).await,
        ) {
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
}

/// CPU-intensive transaction processing, spawned in a blocking thread to avoid blocking other tasks
/// from running.
struct ProcessTxBlockingTask {
    stateless_tx_validator: Arc<dyn StatelessTransactionValidatorTrait>,
    stateful_tx_validator_factory: Arc<dyn StatefulTransactionValidatorFactoryTrait>,
    state_reader_factory: Arc<dyn StateReaderFactory>,
    mempool_client: SharedMempoolClient,
    tx: RpcTransaction,
    internal_tx: InternalRpcTransaction,
    executable_tx: AccountTransaction,
    runtime: tokio::runtime::Handle,
}

impl ProcessTxBlockingTask {
    pub fn new(
        gateway: &Gateway,
        tx: RpcTransaction,
        internal_tx: InternalRpcTransaction,
        executable_tx: AccountTransaction,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            stateless_tx_validator: gateway.stateless_tx_validator.clone(),
            stateful_tx_validator_factory: gateway.stateful_tx_validator_factory.clone(),
            state_reader_factory: gateway.state_reader_factory.clone(),
            mempool_client: gateway.mempool_client.clone(),
            tx,
            internal_tx,
            executable_tx,
            runtime,
        }
    }

    // TODO(Arni): Make into async function and remove all block_on calls once we manage removing
    // the spawn_blocking call.
    fn process_tx(self) -> GatewayResult<AddTransactionArgs> {
        // Perform stateless validations.
        self.stateless_tx_validator.validate(&self.tx)?;

        // let tx_signature = self.tx.signature().clone();
        // let internal_tx = self
        //     .runtime
        //     .block_on(self.transaction_converter.convert_rpc_tx_to_internal_rpc_tx(self.tx))
        //     .map_err(|e| {
        //         warn!("Failed to convert RPC transaction to internal RPC transaction: {}", e);
        //         transaction_converter_err_to_deprecated_gw_err(&tx_signature, e)
        //     })?;

        // let executable_tx = self
        //     .runtime
        //     .block_on(
        //         self.transaction_converter
        //             .convert_internal_rpc_tx_to_executable_tx(internal_tx.clone()),
        //     )
        //     .map_err(|e| {
        //         warn!(
        //             "Failed to convert internal RPC transaction to executable transaction: {}",
        //             e
        //         );
        //         transaction_converter_err_to_deprecated_gw_err(&tx_signature, e)
        //     })?;

        let mut stateful_transaction_validator = self
            .stateful_tx_validator_factory
            .instantiate_validator(self.state_reader_factory.as_ref())?;

        let nonce = stateful_transaction_validator.extract_state_nonce_and_run_validations(
            &self.executable_tx,
            self.mempool_client,
            self.runtime,
        )?;

        Ok(AddTransactionArgs::new(self.internal_tx, nonce))
    }
}

pub fn create_gateway(
    config: GatewayConfig,
    shared_state_sync_client: SharedStateSyncClient,
    mempool_client: SharedMempoolClient,
    class_manager_client: SharedClassManagerClient,
    runtime: tokio::runtime::Handle,
) -> Gateway {
    let state_reader_factory = Arc::new(SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client: class_manager_client.clone(),
        runtime,
    });
    let transaction_converter =
        TransactionConverter::new(class_manager_client, config.chain_info.chain_id.clone());

    Gateway::new(config, state_reader_factory, mempool_client, transaction_converter)
}

#[async_trait]
impl ComponentStarter for Gateway {
    async fn start(&mut self) {
        register_metrics();
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
