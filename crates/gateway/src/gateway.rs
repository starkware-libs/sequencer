use std::clone::Clone;
use std::net::SocketAddr;
use std::panic;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use blockifier::execution::execution_utils::felt_to_stark_felt;
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{RPCDeclareTransaction, RPCTransaction};
use starknet_api::transaction::TransactionHash;
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::{Account, MempoolInput};
use starknet_sierra_compile::compile::{compile_sierra_to_casm, CompilationUtilError};
use starknet_sierra_compile::utils::into_contract_class_for_compilation;

use crate::config::{GatewayConfig, GatewayNetworkConfig, RpcStateReaderConfig};
use crate::errors::{GatewayError, GatewayRunError};
use crate::rpc_state_reader::RpcStateReaderFactory;
use crate::starknet_api_test_utils::get_sender_address;
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::external_tx_to_thin_tx;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

pub type GatewayResult<T> = Result<T, GatewayError>;

pub struct Gateway {
    pub config: GatewayConfig,
    app_state: AppState,
}

#[derive(Clone)]
pub struct AppState {
    pub stateless_tx_validator: StatelessTransactionValidator,
    pub stateful_tx_validator: Arc<StatefulTransactionValidator>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub mempool_client: SharedMempoolClient,
}

impl Gateway {
    pub fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<dyn StateReaderFactory>,
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
            mempool_client,
        };
        Gateway { config, app_state }
    }

    pub async fn run(self) -> Result<(), GatewayRunError> {
        // Parses the bind address from GatewayConfig, returning an error for invalid addresses.
        let GatewayNetworkConfig { ip, port } = self.config.network_config;
        let addr = SocketAddr::new(ip, port);
        let app = self.app();

        // Create a server that runs forever.
        Ok(axum::Server::bind(&addr).serve(app.into_make_service()).await?)
    }

    pub fn app(self) -> Router {
        Router::new()
            .route("/is_alive", get(is_alive))
            .route("/add_tx", post(add_tx))
            .with_state(self.app_state)
    }
}

// Gateway handlers.

async fn is_alive() -> GatewayResult<String> {
    unimplemented!("Future handling should be implemented here.");
}

async fn add_tx(
    State(app_state): State<AppState>,
    Json(tx): Json<RPCTransaction>,
) -> GatewayResult<Json<TransactionHash>> {
    let mempool_input = tokio::task::spawn_blocking(move || {
        process_tx(
            app_state.stateless_tx_validator,
            app_state.stateful_tx_validator.as_ref(),
            app_state.state_reader_factory.as_ref(),
            tx,
        )
    })
    .await??;

    let tx_hash = mempool_input.tx.tx_hash;

    app_state
        .mempool_client
        .add_tx(mempool_input)
        .await
        .map_err(|e| GatewayError::MessageSendError(e.to_string()))?;
    // TODO: Also return `ContractAddress` for deploy and `ClassHash` for Declare.
    Ok(Json(tx_hash))
}

fn process_tx(
    stateless_tx_validator: StatelessTransactionValidator,
    stateful_tx_validator: &StatefulTransactionValidator,
    state_reader_factory: &dyn StateReaderFactory,
    tx: RPCTransaction,
) -> GatewayResult<MempoolInput> {
    // TODO(Arni, 1/5/2024): Perform congestion control.

    // Perform stateless validations.
    stateless_tx_validator.validate(&tx)?;

    // Compile Sierra to Casm.
    let optional_class_info = match &tx {
        RPCTransaction::Declare(declare_tx) => Some(compile_contract_class(declare_tx)?),
        _ => None,
    };

    // TODO(Yael, 19/5/2024): pass the relevant deploy_account_hash.
    let tx_hash =
        stateful_tx_validator.run_validate(state_reader_factory, &tx, optional_class_info, None)?;

    // TODO(Arni): Add the Sierra and the Casm to the mempool input.
    Ok(MempoolInput {
        tx: external_tx_to_thin_tx(&tx, tx_hash),
        account: Account { sender_address: get_sender_address(&tx), ..Default::default() },
    })
}

/// Formats the contract class for compilation, compiles it, and returns the compiled contract class
/// wrapped in a [`ClassInfo`].
/// Assumes the contract class is of a Sierra program which is compiled to Casm.
pub fn compile_contract_class(declare_tx: &RPCDeclareTransaction) -> GatewayResult<ClassInfo> {
    let RPCDeclareTransaction::V3(tx) = declare_tx;
    let starknet_api_contract_class = &tx.contract_class;
    let cairo_lang_contract_class =
        into_contract_class_for_compilation(starknet_api_contract_class);

    // Compile Sierra to Casm.
    let catch_unwind_result =
        panic::catch_unwind(|| compile_sierra_to_casm(cairo_lang_contract_class));
    let casm_contract_class = match catch_unwind_result {
        Ok(compilation_result) => compilation_result?,
        Err(_) => {
            // TODO(Arni): Log the panic.
            return Err(GatewayError::CompilationError(CompilationUtilError::CompilationPanic));
        }
    };

    let hash_result =
        CompiledClassHash(felt_to_stark_felt(&casm_contract_class.compiled_class_hash()));
    if hash_result != tx.compiled_class_hash {
        return Err(GatewayError::CompiledClassHashMismatch {
            supplied: tx.compiled_class_hash,
            hash_result,
        });
    }

    // Convert Casm contract class to Starknet contract class directly.
    let blockifier_contract_class =
        ContractClass::V1(ContractClassV1::try_from(casm_contract_class)?);
    let class_info = ClassInfo::new(
        &blockifier_contract_class,
        starknet_api_contract_class.sierra_program.len(),
        starknet_api_contract_class.abi.len(),
    )?;
    Ok(class_info)
}

pub fn create_gateway(
    config: GatewayConfig,
    rpc_state_reader_config: RpcStateReaderConfig,
    client: SharedMempoolClient,
) -> Gateway {
    let state_reader_factory = Arc::new(RpcStateReaderFactory { config: rpc_state_reader_config });
    Gateway::new(config, state_reader_factory, client)
}
