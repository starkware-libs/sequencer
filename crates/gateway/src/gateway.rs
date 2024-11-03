use std::clone::Clone;
use std::sync::Arc;

use blockifier::context::ChainInfo;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
// use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::{AddTransactionArgsWrapper, SharedMempoolClient};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use tracing::{error, instrument};

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
    pub chain_info: ChainInfo,
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
            chain_info: config.chain_info.clone(),
        };
        Gateway { config, app_state }
    }

    pub async fn add_tx(
        &mut self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<TransactionHash> {
        let app_state = self.app_state.clone();
        internal_add_tx(app_state, tx, p2p_message_metadata).await
    }
}

// TODO(Yair): consider consolidating internal_add_tx into add_tx.

#[instrument(skip(app_state))]
async fn internal_add_tx(
    app_state: AppState,
    tx: RpcTransaction,
    p2p_message_metadata: Option<BroadcastedMessageMetadata>,
) -> GatewayResult<TransactionHash> {
    let add_tx_args = tokio::task::spawn_blocking(move || {
        process_tx(
            app_state.stateless_tx_validator,
            app_state.stateful_tx_validator.as_ref(),
            app_state.state_reader_factory.as_ref(),
            app_state.gateway_compiler,
            &app_state.chain_info,
            tx,
        )
    })
    .await
    .map_err(|join_err| {
        error!("Failed to process tx: {}", join_err);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })??;

    let tx_hash = add_tx_args.tx.tx_hash();

    let add_tx_args = AddTransactionArgsWrapper { args: add_tx_args, p2p_message_metadata };
    app_state.mempool_client.add_tx(add_tx_args).await.map_err(|e| {
        error!("Failed to send tx to mempool: {}", e);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })?;
    // TODO: Also return `ContractAddress` for deploy and `ClassHash` for Declare.
    Ok(tx_hash)
}

fn process_tx(
    _stateless_tx_validator: StatelessTransactionValidator,
    stateful_tx_validator: &StatefulTransactionValidator,
    state_reader_factory: &dyn StateReaderFactory,
    gateway_compiler: GatewayCompiler,
    chain_info: &ChainInfo,
    tx: RpcTransaction,
) -> GatewayResult<AddTransactionArgs> {
    // TODO(Arni, 1/5/2024): Perform congestion control.

    // Perform stateless validations.
    // stateless_tx_validator.validate(&tx)?;

    let executable_tx =
        compile_contract_and_build_executable_tx(tx, &gateway_compiler, &chain_info.chain_id)?;

    // Perfom post compilation validations.
    // if let Transaction::Declare(executable_declare_tx) = &executable_tx {
    //     if !executable_declare_tx.validate_compiled_class_hash() {
    //         return Err(GatewaySpecError::CompiledClassHashMismatch);
    //     }
    // }

    let mut validator =
        stateful_tx_validator.instantiate_validator(state_reader_factory, chain_info)?;
    let address = executable_tx.contract_address();
    let nonce = validator.get_nonce(address).map_err(|e| {
        error!("Failed to get nonce for sender address {}: {}", address, e);
        GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
    })?;

    // stateful_tx_validator.run_validate(&executable_tx, nonce, validator)?;

    // TODO(Arni): Add the Sierra and the Casm to the mempool input.
    Ok(AddTransactionArgs { tx: executable_tx, account_state: AccountState { address, nonce } })
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

impl ComponentStarter for Gateway {}
