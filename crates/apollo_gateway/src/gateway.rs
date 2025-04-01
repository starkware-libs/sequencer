use std::clone::Clone;
use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_gateway_types::errors::GatewaySpecError;
use apollo_gateway_types::gateway_types::{
    DeclareGatewayOutput,
    DeployAccountGatewayOutput,
    GatewayOutput,
    InvokeGatewayOutput,
};
use apollo_mempool_types::communication::{AddTransactionArgsWrapper, SharedMempoolClient};
use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_proc_macros::sequencer_latency_histogram;
use apollo_sequencer_infra::component_definitions::ComponentStarter;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use axum::async_trait;
use blockifier::context::ChainInfo;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcTransaction,
};
use tracing::{debug, error, instrument, warn, Span};

use crate::config::GatewayConfig;
use crate::errors::{mempool_client_result_to_gw_spec_result, GatewayResult};
use crate::metrics::{register_metrics, GatewayMetricHandle, GATEWAY_ADD_TX_LATENCY};
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::sync_state_reader::SyncStateReaderFactory;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

pub struct Gateway {
    pub config: GatewayConfig,
    pub stateless_tx_validator: Arc<StatelessTransactionValidator>,
    pub stateful_tx_validator: Arc<StatefulTransactionValidator>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub mempool_client: SharedMempoolClient,
    pub transaction_converter: TransactionConverter,
    pub chain_info: ChainInfo,
}

impl Gateway {
    pub fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<dyn StateReaderFactory>,
        mempool_client: SharedMempoolClient,
        transaction_converter: TransactionConverter,
    ) -> Self {
        Self {
            config: config.clone(),
            stateless_tx_validator: Arc::new(StatelessTransactionValidator {
                config: config.stateless_tx_validator_config.clone(),
            }),
            stateful_tx_validator: Arc::new(StatefulTransactionValidator {
                config: config.stateful_tx_validator_config.clone(),
            }),
            state_reader_factory,
            mempool_client,
            chain_info: config.chain_info.clone(),
            transaction_converter,
        }
    }

    #[instrument(skip_all, ret)]
    // TODO(Yael): add labels for http/p2p once labels are supported
    #[sequencer_latency_histogram(GATEWAY_ADD_TX_LATENCY, true)]
    pub async fn add_tx(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<GatewayOutput> {
        debug!("Processing tx: {:?}", tx);

        // TODO(noamsp): Return same error as in Python gateway.
        if self.config.block_declare {
            if let RpcTransaction::Declare(_) = &tx {
                return Err(GatewaySpecError::UnexpectedError {
                    data: "Transaction type is temporarily blocked.".to_owned(),
                });
            }
        }

        let mut metric_counters = GatewayMetricHandle::new(&tx, &p2p_message_metadata);
        metric_counters.count_transaction_received();

        let blocking_task = ProcessTxBlockingTask::new(self, tx, tokio::runtime::Handle::current());
        // Run the blocking task in the current span.
        let curr_span = Span::current();
        let add_tx_args =
            tokio::task::spawn_blocking(move || curr_span.in_scope(|| blocking_task.process_tx()))
                .await
                .map_err(|join_err| {
                    error!("Failed to process tx: {}", join_err);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })??;

        let gateway_output = create_gateway_output(&add_tx_args.tx);

        let add_tx_args = AddTransactionArgsWrapper { args: add_tx_args, p2p_message_metadata };
        mempool_client_result_to_gw_spec_result(self.mempool_client.add_tx(add_tx_args).await)?;

        metric_counters.transaction_sent_to_mempool();

        Ok(gateway_output)
    }
}

/// CPU-intensive transaction processing, spawned in a blocking thread to avoid blocking other tasks
/// from running.
struct ProcessTxBlockingTask {
    stateless_tx_validator: Arc<StatelessTransactionValidator>,
    stateful_tx_validator: Arc<StatefulTransactionValidator>,
    state_reader_factory: Arc<dyn StateReaderFactory>,
    mempool_client: SharedMempoolClient,
    chain_info: ChainInfo,
    tx: RpcTransaction,
    transaction_converter: TransactionConverter,
    runtime: tokio::runtime::Handle,
}

impl ProcessTxBlockingTask {
    pub fn new(gateway: &Gateway, tx: RpcTransaction, runtime: tokio::runtime::Handle) -> Self {
        Self {
            stateless_tx_validator: gateway.stateless_tx_validator.clone(),
            stateful_tx_validator: gateway.stateful_tx_validator.clone(),
            state_reader_factory: gateway.state_reader_factory.clone(),
            mempool_client: gateway.mempool_client.clone(),
            chain_info: gateway.chain_info.clone(),
            tx,
            transaction_converter: gateway.transaction_converter.clone(),
            runtime,
        }
    }

    // TODO(Arni): Make into async function and remove all block_on calls once we manage removing
    // the spawn_blocking call.
    fn process_tx(self) -> GatewayResult<AddTransactionArgs> {
        // TODO(Arni, 1/5/2024): Perform congestion control.

        // Perform stateless validations.
        self.stateless_tx_validator.validate(&self.tx)?;

        let internal_tx = self
            .runtime
            .block_on(self.transaction_converter.convert_rpc_tx_to_internal_rpc_tx(self.tx))
            .map_err(|err| {
                warn!("Failed to convert RPC transaction to internal RPC transaction: {}", err);
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?;

        let executable_tx = self
            .runtime
            .block_on(
                self.transaction_converter
                    .convert_internal_rpc_tx_to_executable_tx(internal_tx.clone()),
            )
            .map_err(|err| {
                warn!(
                    "Failed to convert internal RPC transaction to executable transaction: {}",
                    err
                );
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?;

        // Perform post compilation validations.
        if let AccountTransaction::Declare(executable_declare_tx) = &executable_tx {
            if !executable_declare_tx.validate_compiled_class_hash() {
                return Err(GatewaySpecError::CompiledClassHashMismatch);
            }
        }

        let mut validator = self
            .stateful_tx_validator
            .instantiate_validator(self.state_reader_factory.as_ref(), &self.chain_info)?;
        let address = executable_tx.contract_address();
        let nonce = validator.get_nonce(address).map_err(|e| {
            error!("Failed to get nonce for sender address {}: {}", address, e);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })?;

        self.stateful_tx_validator.run_validate(
            &executable_tx,
            nonce,
            self.mempool_client,
            validator,
            self.runtime,
        )?;

        // TODO(Arni): Add the Sierra and the Casm to the mempool input.
        Ok(AddTransactionArgs { tx: internal_tx, account_state: AccountState { address, nonce } })
    }
}

pub fn create_gateway(
    config: GatewayConfig,
    shared_state_sync_client: SharedStateSyncClient,
    mempool_client: SharedMempoolClient,
    class_manager_client: SharedClassManagerClient,
) -> Gateway {
    let state_reader_factory = Arc::new(SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client: class_manager_client.clone(),
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
        InternalRpcTransactionWithoutTxHash::Declare(declare_tx) => {
            GatewayOutput::Declare(DeclareGatewayOutput {
                transaction_hash,
                class_hash: declare_tx.class_hash,
            })
        }
        InternalRpcTransactionWithoutTxHash::DeployAccount(deploy_account_tx) => {
            GatewayOutput::DeployAccount(DeployAccountGatewayOutput {
                transaction_hash,
                address: deploy_account_tx.contract_address,
            })
        }
        InternalRpcTransactionWithoutTxHash::Invoke(_) => {
            GatewayOutput::Invoke(InvokeGatewayOutput { transaction_hash })
        }
    }
}
