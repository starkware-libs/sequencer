use std::clone::Clone;
use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_infra::component_runner::{ComponentStartError, ComponentStarter};
use starknet_mempool_types::communication::SharedMempoolClient;
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

    app_state.mempool_client.add_tx(mempool_input).await.map_err(|e| {
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

    // TODO(Arni): remove copy_of_rpc_tx and use executable_tx directly as the mempool input.
    let copy_of_rpc_tx = tx.clone();
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

    let optional_class_info = match executable_tx {
        starknet_api::executable_transaction::Transaction::Declare(tx) => {
            Some(tx.class_info.try_into().map_err(|e| {
                error!("Failed to convert Starknet API ClassInfo to Blockifier ClassInfo: {:?}", e);
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?)
        }
        _ => None,
    };

    let validator = stateful_tx_validator.instantiate_validator(state_reader_factory)?;
    // TODO(Yael 31/7/24): refactor after IntrnalTransaction is ready, delete validate_info and
    // compute all the info outside of run_validate.
    let validate_info =
        stateful_tx_validator.run_validate(&copy_of_rpc_tx, optional_class_info, validator)?;

    // TODO(Arni): Add the Sierra and the Casm to the mempool input.
    Ok(MempoolInput {
        tx: Transaction::new_from_rpc_tx(
            copy_of_rpc_tx,
            validate_info.tx_hash,
            validate_info.sender_address,
        ),
        account: Account {
            sender_address: validate_info.sender_address,
            state: AccountState { nonce: validate_info.account_nonce },
        },
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
