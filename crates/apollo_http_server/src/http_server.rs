use std::clone::Clone;
use std::net::SocketAddr;
use std::string::String;

use apollo_gateway_types::communication::{GatewayClientError, SharedGatewayClient};
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewayError;
use apollo_gateway_types::gateway_types::{
    GatewayInput,
    GatewayOutput,
    SUPPORTED_TRANSACTION_VERSIONS,
};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{async_trait, Json, Router};
use serde::de::Error;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::serde_utils::bytes_from_hex_str;
use tracing::{debug, info, instrument};

use crate::config::HttpServerConfig;
use crate::deprecated_gateway_transaction::DeprecatedGatewayTransactionV3;
use crate::errors::{HttpServerError, HttpServerRunError};
use crate::metrics::{
    init_metrics,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
};

#[cfg(test)]
#[path = "http_server_test.rs"]
pub mod http_server_test;

pub type HttpServerResult<T> = Result<T, HttpServerError>;

const CLIENT_REGION_HEADER: &str = "X-Client-Region";

pub struct HttpServer {
    pub config: HttpServerConfig,
    app_state: AppState,
}

#[derive(Clone)]
pub struct AppState {
    pub gateway_client: SharedGatewayClient,
}

impl HttpServer {
    pub fn new(config: HttpServerConfig, gateway_client: SharedGatewayClient) -> Self {
        let app_state = AppState { gateway_client };
        HttpServer { config, app_state }
    }

    pub async fn run(&mut self) -> Result<(), HttpServerRunError> {
        init_metrics();

        // Parses the bind address from HttpServerConfig, returning an error for invalid addresses.
        let HttpServerConfig { ip, port } = self.config;
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("HttpServer running using socket: {}", addr);

        // Create a server that runs forever.
        Ok(axum::Server::bind(&addr).serve(app.into_make_service()).await?)
    }

    // TODO(Yael): consider supporting both formats in the same endpoint if possible.
    pub fn app(&self) -> Router {
        Router::new()
            // Json Rpc endpoint
            .route("/gateway/add_rpc_transaction", post(add_rpc_tx))
            .with_state(self.app_state.clone())
            // Rest api endpoint
            .route("/gateway/add_transaction", post(add_tx))
            .with_state(self.app_state.clone())
            // TODO(shahak): Remove this once we fix the centralized simulator to not use is_alive
            // and is_ready.
            .route(
                "/gateway/is_alive",
                get(|| futures::future::ready("Gateway is alive".to_owned()))
            )
            .route(
                "/gateway/is_ready",
                get(|| futures::future::ready("Gateway is ready".to_owned()))
            )
    }
}

// HttpServer handlers.

#[instrument(skip(app_state))]
async fn add_rpc_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<GatewayOutput>> {
    ADDED_TRANSACTIONS_TOTAL.increment(1);
    add_tx_inner(app_state, headers, tx).await
}

#[instrument(skip(app_state))]
async fn add_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    tx: String,
) -> HttpServerResult<Json<GatewayOutput>> {
    ADDED_TRANSACTIONS_TOTAL.increment(1);
    validate_supported_tx_version(&tx).inspect_err(|e| {
        debug!("Error while validating transaction version: {}", e);
        increment_failure_metrics(e);
    })?;
    let tx: DeprecatedGatewayTransactionV3 = serde_json::from_str(&tx).inspect_err(|e| {
        debug!("Error while parsing transaction: {}", e);
        ADDED_TRANSACTIONS_FAILURE.increment(1);
    })?;
    let rpc_tx = tx.try_into().inspect_err(|e| {
        debug!("Error while converting deprecated gateway transaction into RPC transaction: {}", e);
    })?;

    add_tx_inner(app_state, headers, rpc_tx).await
}

fn validate_supported_tx_version(tx: &str) -> HttpServerResult<()> {
    let tx_json_value: serde_json::Value = serde_json::from_str(tx)?;
    let tx_version_json = tx_json_value
        .get("version")
        .ok_or_else(|| serde_json::Error::custom("Missing version field"))?;
    let tx_version = tx_version_json
        .as_str()
        .ok_or_else(|| serde_json::Error::custom("Version field is not valid"))?;
    let tx_version =
        u64::from_be_bytes(bytes_from_hex_str::<8, true>(tx_version).map_err(|_| {
            serde_json::Error::custom(format!(
                "Version field is not a valid hex string: {tx_version}"
            ))
        })?);
    if !SUPPORTED_TRANSACTION_VERSIONS.contains(&tx_version) {
        return Err(HttpServerError::GatewayClientError(Box::new(
            GatewayClientError::GatewayError(GatewayError::DeprecatedGatewayError {
                source: StarknetError {
                    code: StarknetErrorCode::KnownErrorCode(
                        KnownStarknetErrorCode::InvalidTransactionVersion,
                    ),
                    message: format!(
                        "Transaction version {tx_version} is not supported. Supported versions: \
                         {SUPPORTED_TRANSACTION_VERSIONS:?}."
                    ),
                },
                p2p_message_metadata: None,
            }),
        )));
    }
    Ok(())
}

async fn add_tx_inner(
    app_state: AppState,
    headers: HeaderMap,
    tx: RpcTransaction,
) -> HttpServerResult<Json<GatewayOutput>> {
    let gateway_input: GatewayInput = GatewayInput { rpc_tx: tx, message_metadata: None };
    let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|e| {
        debug!("Error while adding transaction: {}", e);
        HttpServerError::from(Box::new(e))
    });

    let region =
        headers.get(CLIENT_REGION_HEADER).and_then(|region| region.to_str().ok()).unwrap_or("N/A");
    record_added_transactions(&add_tx_result, region);
    Ok(Json(add_tx_result?))
}

fn record_added_transactions(add_tx_result: &HttpServerResult<GatewayOutput>, region: &str) {
    match add_tx_result {
        Ok(gateway_output) => {
            // TODO(Arni): Reconsider the tracing level for this log.
            info!(
                transaction_hash = %gateway_output.transaction_hash(),
                region = %region,
                "Recorded transaction"
            );
            ADDED_TRANSACTIONS_SUCCESS.increment(1);
        }
        Err(err) => increment_failure_metrics(err),
    }
}

pub fn create_http_server(
    config: HttpServerConfig,
    gateway_client: SharedGatewayClient,
) -> HttpServer {
    HttpServer::new(config, gateway_client)
}

#[async_trait]
impl ComponentStarter for HttpServer {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start HttpServer component: {e:?}"))
    }
}

fn increment_failure_metrics(err: &HttpServerError) {
    ADDED_TRANSACTIONS_FAILURE.increment(1);
    let HttpServerError::GatewayClientError(gateway_client_error) = err else {
        return;
    };
    // TODO(shahak): add unit test for ADDED_TRANSACTIONS_INTERNAL_ERROR
    if matches!(&**gateway_client_error, GatewayClientError::ClientError(_))
        || matches!(&**gateway_client_error, GatewayClientError::GatewayError(
            GatewayError::DeprecatedGatewayError { source, .. }) if source.is_internal())
    {
        ADDED_TRANSACTIONS_INTERNAL_ERROR.increment(1);
    }
}
