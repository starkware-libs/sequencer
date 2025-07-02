use std::clone::Clone;
use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
};
use apollo_class_manager_types::SharedClassManagerClient;
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
use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_proc_macros::sequencer_latency_histogram;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use axum::async_trait;
use blockifier::context::ChainInfo;
use num_rational::Ratio;
use starknet_api::block::NonzeroGasPrice;
use starknet_api::executable_transaction::ValidateCompiledClassHashError;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcTransaction,
};
use starknet_api::transaction::fields::ValidResourceBounds;
use tracing::{debug, error, info, instrument, warn, Span};

use crate::config::GatewayConfig;
use crate::errors::{mempool_client_result_to_deprecated_gw_result, GatewayResult};
use crate::metrics::{register_metrics, GatewayMetricHandle, GATEWAY_ADD_TX_LATENCY};
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::sync_state_reader::SyncStateReaderFactory;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

// TODO(Arni): Move to a config.
// Minimum gas price as percentage of threshold to accept transactions.
const MIN_GAS_PRICE_PRECENTAGE: u8 = 80; // E.g., 80 to require 80% of threshold.

#[derive(Clone)]
pub struct Gateway {
    pub config: Arc<GatewayConfig>,
    pub stateless_tx_validator: Arc<StatelessTransactionValidator>,
    pub stateful_tx_validator: Arc<StatefulTransactionValidator>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub mempool_client: SharedMempoolClient,
    pub transaction_converter: Arc<TransactionConverter>,
    pub chain_info: Arc<ChainInfo>,
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
            stateful_tx_validator: Arc::new(StatefulTransactionValidator {
                config: config.stateful_tx_validator_config.clone(),
            }),
            state_reader_factory,
            mempool_client,
            chain_info: Arc::new(config.chain_info.clone()),
            transaction_converter: Arc::new(transaction_converter),
        }
    }

    #[instrument(skip_all, fields(is_p2p = p2p_message_metadata.is_some()), ret)]
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
                return Err(StarknetError {
                    code: StarknetErrorCode::UnknownErrorCode(
                        "StarknetErrorCode.BLOCKED_TRANSACTION_TYPE".to_string(),
                    ),
                    message: "Transaction type is temporarily blocked.".to_string(),
                });
            }
        }

        let mut metric_counters = GatewayMetricHandle::new(&tx, &p2p_message_metadata);
        metric_counters.count_transaction_received();

        let blocking_task =
            ProcessTxBlockingTask::new(self, tx.clone(), tokio::runtime::Handle::current());
        // Run the blocking task in the current span.
        let curr_span = Span::current();
        let add_tx_args =
            tokio::task::spawn_blocking(move || curr_span.in_scope(|| blocking_task.process_tx()))
                .await
                .map_err(|join_err| {
                    error!("Failed to process tx: {}", join_err);
                    StarknetError::internal(&join_err.to_string())
                })?
                .inspect_err(|starknet_error| {
                    info!(
                        "Gateway validation failed for tx: {:?} with error: {}",
                        tx, starknet_error
                    );
                })?;

        let gateway_output = create_gateway_output(&add_tx_args.tx);

        let add_tx_args = AddTransactionArgsWrapper { args: add_tx_args, p2p_message_metadata };
        mempool_client_result_to_deprecated_gw_result(
            self.mempool_client.add_tx(add_tx_args).await,
        )?;

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
    chain_info: Arc<ChainInfo>,
    tx: RpcTransaction,
    transaction_converter: Arc<TransactionConverter>,
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
            .map_err(|e| {
                warn!("Failed to convert RPC transaction to internal RPC transaction: {}", e);
                match e {
                    TransactionConverterError::ValidateCompiledClassHashError(err) => {
                        convert_compiled_class_hash_error(err)
                    }
                    other => {
                        // TODO(yair): Fix this. Need to map the errors better.
                        StarknetError::internal(&other.to_string())
                    }
                }
            })?;

        let executable_tx = self
            .runtime
            .block_on(
                self.transaction_converter
                    .convert_internal_rpc_tx_to_executable_tx(internal_tx.clone()),
            )
            .map_err(|e| {
                warn!(
                    "Failed to convert internal RPC transaction to executable transaction: {}",
                    e
                );
                // TODO(yair): Fix this.
                StarknetError::internal(&e.to_string())
            })?;

        let mut validator = self
            .stateful_tx_validator
            .instantiate_validator(self.state_reader_factory.as_ref(), &self.chain_info)?;

        // Skip this validation during the systems bootstrap phase.
        if self.stateless_tx_validator.config.validate_non_zero_resource_bounds {
            let l2_gas_price =
                validator.block_context().block_info().gas_prices.strk_gas_prices.l2_gas_price;
            validate_tx_l2_gas_price_within_threshold(
                executable_tx.resource_bounds(),
                l2_gas_price,
            )?;
        }

        let address = executable_tx.contract_address();
        let nonce = validator.get_nonce(address).map_err(|e| {
            error!("Failed to get nonce for sender address {}: {}", address, e);
            // TODO(yair): Fix this. Need to map the errors better.
            StarknetError::internal(&e.to_string())
        })?;

        self.stateful_tx_validator
            .run_validate(&executable_tx, nonce, self.mempool_client, validator, self.runtime)
            .map_err(|e| StarknetError {
                code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
                message: e.to_string(),
            })?;

        // TODO(Arni): Add the Sierra and the Casm to the mempool input.
        Ok(AddTransactionArgs { tx: internal_tx, account_state: AccountState { address, nonce } })
    }
}

// TODO(Arni): Consider running this validation for all gas prices.
fn validate_tx_l2_gas_price_within_threshold(
    tx_resource_bounds: ValidResourceBounds,
    previous_block_l2_gas_price: NonzeroGasPrice,
) -> GatewayResult<()> {
    match tx_resource_bounds {
        ValidResourceBounds::AllResources(tx_resource_bounds) => {
            let tx_l2_gas_price = tx_resource_bounds.l2_gas.max_price_per_unit;
            let gas_price_threshold_multiplier =
                Ratio::new(MIN_GAS_PRICE_PRECENTAGE.into(), 100_u128);
            let threshold =
                (gas_price_threshold_multiplier * previous_block_l2_gas_price.get().0).to_integer();
            if tx_l2_gas_price.0 < threshold {
                return Err(StarknetError {
                    // We didn't have this kind of an error.
                    code: StarknetErrorCode::UnknownErrorCode(
                        "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
                    ),
                    message: format!(
                        "Transaction gas price {} is below the required threshold {}.",
                        tx_l2_gas_price, threshold
                    ),
                });
            }
        }
        ValidResourceBounds::L1Gas(_) => {
            // No validation required for legacy transactions.
        }
    }

    Ok(())
}

fn convert_compiled_class_hash_error(error: ValidateCompiledClassHashError) -> StarknetError {
    let ValidateCompiledClassHashError::CompiledClassHashMismatch {
        computed_class_hash,
        supplied_class_hash,
    } = error;
    StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.INVALID_COMPILED_CLASS_HASH".to_string(),
        ),
        message: format!(
            "Computed compiled class hash: {computed_class_hash} does not match the given value: \
             {supplied_class_hash}.",
        ),
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
