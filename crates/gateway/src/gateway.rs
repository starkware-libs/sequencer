use std::clone::Clone;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use starknet_api::rpc_transaction::RPCTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool_infra::component_runner::{ComponentRunner, ComponentStartError};
use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::{Account, MempoolInput};
use tracing::info;

use crate::compilation::compile_contract_class;
use crate::config::{GatewayConfig, GatewayNetworkConfig, RpcStateReaderConfig};
use crate::errors::{GatewayError, GatewayResult, GatewayRunError};
use crate::rpc_state_reader::RpcStateReaderFactory;
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::{external_tx_to_thin_tx, get_sender_address};

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

    pub async fn run(&mut self) -> Result<(), GatewayRunError> {
        // Parses the bind address from GatewayConfig, returning an error for invalid addresses.
        let GatewayNetworkConfig { ip, port } = self.config.network_config;
        let addr = SocketAddr::new(ip, port);
        let app = self.app();

        // Create a server that runs forever.
        Ok(axum::Server::bind(&addr).serve(app.into_make_service()).await?)
    }

    pub fn app(&self) -> Router {
        Router::new()
            .route("/is_alive", get(is_alive))
            .route("/add_tx", post(add_tx))
            .with_state(self.app_state.clone())
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
        stateful_tx_validator.run_validate(state_reader_factory, &tx, optional_class_info)?;

    // TODO(Arni): Add the Sierra and the Casm to the mempool input.
    Ok(MempoolInput {
        tx: external_tx_to_thin_tx(&tx, tx_hash),
        account: Account { sender_address: get_sender_address(&tx), ..Default::default() },
    })
}

pub fn create_gateway(
    config: GatewayConfig,
    rpc_state_reader_config: RpcStateReaderConfig,
    client: SharedMempoolClient,
) -> Gateway {
    let state_reader_factory = Arc::new(RpcStateReaderFactory { config: rpc_state_reader_config });
    Gateway::new(config, state_reader_factory, client)
}

#[async_trait]
impl ComponentRunner for Gateway {
    async fn start(&mut self) -> Result<(), ComponentStartError> {
        info!("Gateway::start()");
        self.run().await.map_err(|_| ComponentStartError::InternalComponentError)
    }
}
