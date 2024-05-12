use std::net::SocketAddr;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use starknet_api::external_transaction::ExternalTransaction;
use starknet_mempool_types::mempool_types::GatewayNetworkComponent;

use crate::config::GatewayNetworkConfig;
use crate::errors::GatewayError;
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator, StatelessTransactionValidatorConfig,
};

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

pub type GatewayResult<T> = Result<T, GatewayError>;

pub struct Gateway {
    pub network_config: GatewayNetworkConfig,
    // TODO(Arni, 7/5/2024): Move the stateless transaction validator config into the gateway
    // config.
    pub stateless_transaction_validator_config: StatelessTransactionValidatorConfig,
    pub network: GatewayNetworkComponent,
}

#[derive(Clone)]
pub struct GatewayState {
    pub stateless_transaction_validator: StatelessTransactionValidator,
}

impl Gateway {
    pub async fn build_server(self) {
        // Parses the bind address from GatewayConfig, returning an error for invalid addresses.
        let addr = SocketAddr::new(self.network_config.ip, self.network_config.port);
        let app = app(self.stateless_transaction_validator_config);

        // Create a server that runs forever.
        axum::Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
    }
}

// TODO(Arni, 7/5/2024): Change this function to accept GatewayConfig.
/// Sets up the router with the specified routes for the server.
pub fn app(config: StatelessTransactionValidatorConfig) -> Router {
    let gateway_state =
        GatewayState { stateless_transaction_validator: StatelessTransactionValidator { config } };

    Router::new()
        .route("/is_alive", get(is_alive))
        .route("/add_transaction", post(async_add_transaction))
        .with_state(gateway_state)
    // TODO: when we need to configure the router, like adding banned ips, add it here via
    // `with_state`.
}

async fn is_alive() -> GatewayResult<String> {
    unimplemented!("Future handling should be implemented here.");
}

async fn async_add_transaction(
    State(gateway_state): State<GatewayState>,
    Json(transaction): Json<ExternalTransaction>,
) -> GatewayResult<String> {
    tokio::task::spawn_blocking(move || add_transaction(gateway_state, transaction)).await?
}

fn add_transaction(
    gateway_state: GatewayState,
    transaction: ExternalTransaction,
) -> GatewayResult<String> {
    // TODO(Arni, 1/5/2024): Preform congestion control.

    // Perform stateless validations.
    gateway_state.stateless_transaction_validator.validate(&transaction)?;

    // TODO(Yael, 1/5/2024): Preform state related validations.
    // TODO(Arni, 1/5/2024): Move transaction to mempool.

    // TODO(Arni, 1/5/2024): Produce response.
    // Send response.
    Ok(match transaction {
        ExternalTransaction::Declare(_) => "DECLARE".into(),
        ExternalTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT".into(),
        ExternalTransaction::Invoke(_) => "INVOKE".into(),
    })
}
