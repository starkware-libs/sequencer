use std::clone::Clone;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use mempool_infra::network_component::CommunicationInterface;
use starknet_api::external_transaction::ExternalTransaction;
use starknet_mempool_types::mempool_types::{
    Account, GatewayNetworkComponent, GatewayToMempoolMessage, MempoolInput,
};

use crate::config::{
    GatewayNetworkConfig, StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig,
};
use crate::errors::GatewayError;
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
    pub network_config: GatewayNetworkConfig,
    // TODO(Arni, 7/5/2024): Move the stateless transaction validator config into the gateway
    // config.
    pub stateless_transaction_validator_config: StatelessTransactionValidatorConfig,
    pub stateful_transaction_validator_config: StatefulTransactionValidatorConfig,
    pub network_component: GatewayNetworkComponent,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
}

#[derive(Clone)]
pub struct AppState {
    pub stateless_transaction_validator: StatelessTransactionValidator,
    pub stateful_transaction_validator: Arc<StatefulTransactionValidator>,
    /// This field uses Arc to enable shared ownership, which is necessary because
    /// `GatewayNetworkClient` supports only one receiver at a time.
    pub network_component: Arc<GatewayNetworkComponent>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
}

impl Gateway {
    pub async fn build_server(self) {
        // Parses the bind address from GatewayConfig, returning an error for invalid addresses.
        let addr = SocketAddr::new(self.network_config.ip, self.network_config.port);
        let app = self.app();

        // Create a server that runs forever.
        axum::Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
    }

    pub fn app(self) -> Router {
        let app_state = AppState {
            stateless_transaction_validator: StatelessTransactionValidator {
                config: self.stateless_transaction_validator_config,
            },
            stateful_transaction_validator: Arc::new(StatefulTransactionValidator {
                config: self.stateful_transaction_validator_config,
            }),
            network_component: Arc::new(self.network_component),
            state_reader_factory: self.state_reader_factory,
        };

        Router::new()
            .route("/is_alive", get(is_alive))
            .route("/add_tx", post(add_tx))
            .with_state(app_state)
        // TODO: when we need to configure the router, like adding banned ips, add it here via
        // `with_state`.
    }
}

// Gateway handlers.

async fn is_alive() -> GatewayResult<String> {
    unimplemented!("Future handling should be implemented here.");
}

async fn add_tx(
    State(app_state): State<AppState>,
    Json(tx): Json<ExternalTransaction>,
) -> GatewayResult<String> {
    let (response, mempool_input) = tokio::task::spawn_blocking(move || {
        process_tx(
            app_state.stateless_transaction_validator,
            app_state.stateful_transaction_validator.as_ref(),
            app_state.state_reader_factory.as_ref(),
            tx,
        )
    })
    .await??;

    let message = GatewayToMempoolMessage::AddTransaction(mempool_input);
    app_state
        .network_component
        .send(message)
        .await
        .map_err(|e| GatewayError::MessageSendError(e.to_string()))?;
    Ok(response)
}

fn process_tx(
    stateless_transaction_validator: StatelessTransactionValidator,
    stateful_transaction_validator: &StatefulTransactionValidator,
    state_reader_factory: &dyn StateReaderFactory,
    tx: ExternalTransaction,
) -> GatewayResult<(String, MempoolInput)> {
    // TODO(Arni, 1/5/2024): Preform congestion control.

    // Perform stateless validations.
    stateless_transaction_validator.validate(&tx)?;

    // TODO(Yael, 19/5/2024): pass the relevant class_info and deploy_account_hash.
    stateful_transaction_validator.run_validate(state_reader_factory, &tx, None, None)?;
    // TODO(Yael, 19/5/2024): return the tx_hash.
    // TODO(Arni, 1/5/2024): Move transaction to mempool.

    // TODO(Arni, 1/5/2024): Produce response.
    // Send response.

    let mempool_input = MempoolInput {
        tx: external_tx_to_thin_tx(&tx),
        account: Account { address: get_sender_address(&tx), ..Default::default() },
    };

    Ok((
        match tx {
            ExternalTransaction::Declare(_) => "DECLARE".into(),
            ExternalTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT".into(),
            ExternalTransaction::Invoke(_) => "INVOKE".into(),
        },
        mempool_input,
    ))
}
