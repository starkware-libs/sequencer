use std::clone::Clone;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
use starknet_mempool_types::communication::{MempoolWrapperInput, SharedMempoolClient};
use starknet_mempool_types::mempool_types::{Account, AccountState, MempoolInput};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use tracing::{error, info, instrument};

use crate::compilation::GatewayCompiler;
use crate::config::{GatewayConfig, RpcStateReaderConfig};
use crate::errors::GatewayResult;
use crate::rpc_state_reader::RpcStateReaderFactory;
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::compile_contract_and_build_executable_tx;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

// TODO(yair): remove the usage of app_state.

pub struct Gateway {
    pub config: GatewayConfig,
    app_state: AppState,
}

#[derive(Clone)]
pub struct AppState {
    pub stateless_tx_validator: StatelessTransactionValidator,
    pub stateful_tx_validator: Arc<StatefulTransactionValidator>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub gateway_compiler: GatewayCompiler,
    pub mempool_client: SharedMempoolClient,
}

impl Gateway {
    pub fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<dyn StateReaderFactory>,
        gateway_compiler: GatewayCompiler,
        mempool_client: SharedMempoolClient,
    ) -> Self {
        let app_state = AppState {
            stateless_tx_validator: StatelessTransactionValidator {
                config: config.stateless_tx_validator_config.clone(),
            },
            stateful_tx_validator: Arc::new(StatefulTransactionValidator {
                config: config.stateful_tx_validator_config.clone(),
            }),
            state_reader_factory,
            gateway_compiler,
            mempool_client,
        };
        Gateway { config, app_state }
    }

    pub async fn add_tx(&mut self, tx: RpcTransaction) -> GatewayResult<TransactionHash> {
        let app_state = self.app_state.clone();
        internal_add_tx(app_state, tx).await
    }
}

// TODO(Tsabary/yair): consider consolidating internal_add_tx into add_tx.

#[instrument(skip(app_state))]
async fn internal_add_tx(
    app_state: AppState,
    tx: RpcTransaction,
) -> GatewayResult<TransactionHash> {
    let mempool_input = tokio::task::spawn_blocking(move || {
        process_tx(
            app_state.stateless_tx_validator,
            app_state.stateful_tx_validator.as_ref(),
            app_state.state_reader_factory.as_ref(),
            app_state.gateway_compiler,
            tx,
        )
    })
    .await
    .map_err(|join_err| {
        error!("Failed to process tx: {}", join_err);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })??;

    let tx_hash = mempool_input.tx.tx_hash();

    let mempool_wrapper_input = MempoolWrapperInput { mempool_input, message_metadata: None };
    app_state.mempool_client.add_tx(mempool_wrapper_input).await.map_err(|e| {
        error!("Failed to send tx to mempool: {}", e);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })?;
    // TODO: Also return `ContractAddress` for deploy and `ClassHash` for Declare.
    Ok(tx_hash)
}

fn process_tx(
    stateless_tx_validator: StatelessTransactionValidator,
    stateful_tx_validator: &StatefulTransactionValidator,
    state_reader_factory: &dyn StateReaderFactory,
    gateway_compiler: GatewayCompiler,
    tx: RpcTransaction,
) -> GatewayResult<MempoolInput> {
    // TODO(Arni, 1/5/2024): Perform congestion control.

    // Perform stateless validations.
    stateless_tx_validator.validate(&tx)?;

    let executable_tx = compile_contract_and_build_executable_tx(
        tx,
        &gateway_compiler,
        &stateful_tx_validator.config.chain_info.chain_id,
    )?;

    // Perfom post compilation validations.
    if let Transaction::Declare(executable_declare_tx) = &executable_tx {
        if !executable_declare_tx.validate_compiled_class_hash() {
            return Err(GatewaySpecError::CompiledClassHashMismatch);
        }
    }

    let mut validator = stateful_tx_validator.instantiate_validator(state_reader_factory)?;
    let sender_address = executable_tx.contract_address();
    let account_nonce = validator.get_nonce(sender_address).map_err(|e| {
        error!("Failed to get nonce for sender address {}: {}", sender_address, e);
        GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
    })?;

    stateful_tx_validator.run_validate(&executable_tx, account_nonce, validator)?;

    // TODO(Arni): Add the Sierra and the Casm to the mempool input.
    Ok(MempoolInput {
        tx: executable_tx,
        account: Account { sender_address, state: AccountState { nonce: account_nonce } },
    })
}

pub fn create_gateway(
    config: GatewayConfig,
    rpc_state_reader_config: RpcStateReaderConfig,
    compiler_config: SierraToCasmCompilationConfig,
    mempool_client: SharedMempoolClient,
) -> Gateway {
    let state_reader_factory = Arc::new(RpcStateReaderFactory { config: rpc_state_reader_config });
    let gateway_compiler = GatewayCompiler::new_command_line_compiler(compiler_config);

    Gateway::new(config, state_reader_factory, gateway_compiler, mempool_client)
}

#[async_trait]
impl ComponentStarter for Gateway {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        info!("Gateway::start()");
        Ok(())
    }
}
