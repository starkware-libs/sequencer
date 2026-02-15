use std::clone::Clone;
use std::net::SocketAddr;
use std::string::String;
use std::time::Duration;

use apollo_config_manager_types::communication::SharedConfigManagerClient;
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
use apollo_http_server_config::config::{HttpServerConfig, HttpServerDynamicConfig};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use apollo_proc_macros::sequencer_latency_histogram;
use async_trait::async_trait;
use axum::extract::DefaultBodyLimit;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{serve, Extension, Json, Router};
use blockifier_reexecution::serde_utils::deserialize_transaction_json_to_starknet_api_tx;
use serde::de::Error;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::transaction::fields::ValidResourceBounds;
use tokio::net::TcpListener;
use tokio::sync::watch::{channel, Receiver, Sender};
use tokio::time;
use tracing::{debug, info, instrument, warn};

use crate::deprecated_gateway_transaction::DeprecatedGatewayTransactionV3;
use crate::errors::{HttpServerError, HttpServerRunError};
use crate::metrics::{
    init_metrics,
    ADDED_TRANSACTIONS_DEPRECATED_ERROR,
    ADDED_TRANSACTIONS_FAILURE,
    ADDED_TRANSACTIONS_INTERNAL_ERROR,
    ADDED_TRANSACTIONS_SUCCESS,
    ADDED_TRANSACTIONS_TOTAL,
    HTTP_SERVER_ADD_TX_LATENCY,
};

#[cfg(test)]
#[path = "http_server_test.rs"]
pub mod http_server_test;

pub type HttpServerResult<T> = Result<T, HttpServerError>;

const CLIENT_REGION_HEADER: &str = "X-Client-Region";

pub struct HttpServer {
    config: HttpServerConfig,
    app_state: AppState,
    config_manager_client: SharedConfigManagerClient,
    dynamic_config_tx: Sender<HttpServerDynamicConfig>,
}

#[derive(Clone)]
pub struct AppState {
    gateway_client: SharedGatewayClient,
    dynamic_config_rx: Receiver<HttpServerDynamicConfig>,
}

impl AppState {
    fn get_dynamic_config(&self) -> HttpServerDynamicConfig {
        // `borrow()` returns a reference to the value owned by the channel, hence we clone it.
        let config = {
            let config = self.dynamic_config_rx.borrow();
            config.clone()
        };
        config
    }
}

impl HttpServer {
    pub fn new(
        config: HttpServerConfig,
        config_manager_client: SharedConfigManagerClient,
        gateway_client: SharedGatewayClient,
    ) -> Self {
        let (dynamic_config_tx, dynamic_config_rx) =
            channel::<HttpServerDynamicConfig>(config.dynamic_config.clone());
        let app_state = AppState { gateway_client, dynamic_config_rx };
        HttpServer { config, app_state, config_manager_client, dynamic_config_tx }
    }

    pub async fn run(&mut self) -> Result<(), HttpServerRunError> {
        init_metrics();

        // Parses the bind address from HttpServerConfig, returning an error for invalid addresses.
        let (ip, port) = self.config.ip_and_port();
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("HttpServer running using socket: {}", addr);

        tokio::spawn(dynamic_config_poll(
            self.dynamic_config_tx.clone(),
            self.config_manager_client.clone(),
            self.config.static_config.dynamic_config_poll_interval,
        ));

        // TODO(Tsabary): update the http server struct to hold optional fields of the
        // dynamic_config_tx, config_manager_client, and a JoinHandle for the polling task.
        // Then, use `set` and `take` to move these around as needed.

        // Create a server that runs forever.
        let listener = TcpListener::bind(&addr).await?;
        Ok(serve(listener, app).await?)
    }

    // TODO(Yael): consider supporting both formats in the same endpoint if possible.
    pub fn app(&self) -> Router {
        Router::new()
            // Json Rpc endpoint
            .route("/gateway/add_rpc_transaction", post(add_rpc_tx))
            // Rest api endpoint
            .route("/gateway/add_transaction", post(add_tx))
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
            .layer(Extension(self.app_state.clone()))
            .layer(DefaultBodyLimit::disable())
    }
}

// HttpServer handlers.

#[instrument(skip(app_state))]
async fn add_rpc_tx(
    Extension(app_state): Extension<AppState>,
    headers: HeaderMap,
    Json(tx): Json<RpcTransaction>,
) -> HttpServerResult<Json<GatewayOutput>> {
    debug!("ADD_TX_START: Http server received a new transaction.");

    let HttpServerDynamicConfig { accept_new_txs, .. } = app_state.get_dynamic_config();
    check_new_transactions_are_allowed(accept_new_txs)?;

    ADDED_TRANSACTIONS_TOTAL.increment(1);
    add_tx_inner(app_state, headers, tx).await
}

#[instrument(skip(app_state))]
#[sequencer_latency_histogram(HTTP_SERVER_ADD_TX_LATENCY, true)]
async fn add_tx(
    Extension(app_state): Extension<AppState>,
    headers: HeaderMap,
    tx: String,
) -> HttpServerResult<Json<GatewayOutput>> {
    debug!("ADD_TX_START: Http server received a new transaction.");

    let HttpServerDynamicConfig { accept_new_txs, max_sierra_program_size } =
        app_state.get_dynamic_config();
    check_new_transactions_are_allowed(accept_new_txs)?;

    ADDED_TRANSACTIONS_TOTAL.increment(1);
    let tx: DeprecatedGatewayTransactionV3 = match serde_json::from_str(&tx) {
        Ok(value) => value,
        Err(e) => {
            validate_supported_tx_version_str(&tx).inspect_err(|e| {
                debug!("Error while validating transaction version: {}", e);
                increment_failure_metrics(e);
            })?;

            debug!("Error while parsing transaction: {}", e);
            check_supported_resource_bounds_and_increment_metrics(&tx);
            return Err(e.into());
        }
    };

    let rpc_tx = tx.convert_to_rpc_tx(max_sierra_program_size).inspect_err(|e| {
        debug!("Error while converting deprecated gateway transaction into RPC transaction: {}", e);
    })?;

    add_tx_inner(app_state, headers, rpc_tx).await
}

fn check_new_transactions_are_allowed(accept_new_txs: bool) -> HttpServerResult<()> {
    match accept_new_txs {
        true => Ok(()),
        false => Err(HttpServerError::DisabledError()),
    }
}

#[allow(clippy::result_large_err)]
fn validate_supported_tx_version_str(tx: &str) -> HttpServerResult<()> {
    // 1. Remove all whitespace
    let mut compact = String::with_capacity(tx.len());
    compact.extend(tx.chars().filter(|c| !c.is_whitespace()));

    // 2. Find version:" marker
    let marker = "\"version\":\"";
    let start =
        compact.find(marker).ok_or_else(|| serde_json::Error::custom("Missing version field"))?;
    let rest = &compact[start + marker.len()..];

    // 3. Find closing quote
    let end = rest.find('"').ok_or_else(|| serde_json::Error::custom("Missing version field"))?;
    let tx_version_str = &rest[..end];

    // 4. Parse version hex string
    let tx_version =
        u64::from_be_bytes(bytes_from_hex_str::<8, true>(tx_version_str).map_err(|_| {
            serde_json::Error::custom(format!(
                "Version field is not a valid hex string: {tx_version_str}"
            ))
        })?);

    // 5. Handle version errors as before
    handle_tx_version_error(&tx_version)
}

fn handle_tx_version_error(tx_version: &u64) -> HttpServerResult<()> {
    if !SUPPORTED_TRANSACTION_VERSIONS.contains(tx_version) {
        ADDED_TRANSACTIONS_DEPRECATED_ERROR.increment(1);
        Err(HttpServerError::GatewayClientError(Box::new(GatewayClientError::GatewayError(
            GatewayError::DeprecatedGatewayError {
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
            },
        ))))
    } else {
        Ok(())
    }
}

fn check_supported_resource_bounds_and_increment_metrics(tx: &str) {
    if let Ok(tx_json_value) = serde_json::from_str(tx) {
        if let Ok(transaction) = deserialize_transaction_json_to_starknet_api_tx(tx_json_value) {
            if let Some(ValidResourceBounds::L1Gas(_)) = transaction.resource_bounds() {
                ADDED_TRANSACTIONS_DEPRECATED_ERROR.increment(1);
            }
        }
    }
    ADDED_TRANSACTIONS_FAILURE.increment(1);
}

async fn add_tx_inner(
    app_state: AppState,
    headers: HeaderMap,
    tx: RpcTransaction,
) -> HttpServerResult<Json<GatewayOutput>> {
    let gateway_input: GatewayInput = GatewayInput { rpc_tx: tx, message_metadata: None };
    // Wrap the gateway client interaction with a tokio::spawn as it is NOT cancel-safe.
    // Even if the current task is cancelled, e.g., when a request is dropped while still being
    // processed, the inner task will continue to run.
    let region = headers
        .get(CLIENT_REGION_HEADER)
        .and_then(|region| region.to_str().ok())
        .unwrap_or("N/A")
        .to_string();
    let add_tx_result = tokio::spawn(async move {
        let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|e| {
            debug!("Error while adding transaction: {}", e);
            HttpServerError::from(Box::new(e))
        });
        record_added_transactions(&add_tx_result, &region);
        add_tx_result
    })
    .await
    .expect("Should be able to get add_tx result");

    Ok(Json(add_tx_result?))
}

fn record_added_transactions(add_tx_result: &HttpServerResult<GatewayOutput>, region: &str) {
    match add_tx_result {
        Ok(gateway_output) => {
            info!(
                transaction_hash = %gateway_output.transaction_hash(),
                region = %region,
                "Recorded transaction"
            );
            ADDED_TRANSACTIONS_SUCCESS.increment(1);
        }
        Err(err) => {
            warn!(
                error = %err,
                "Failed to record transaction"
            );
            increment_failure_metrics(err);
        }
    }
}

pub fn create_http_server(
    config: HttpServerConfig,
    config_manager_client: SharedConfigManagerClient,
    gateway_client: SharedGatewayClient,
) -> HttpServer {
    HttpServer::new(config, config_manager_client, gateway_client)
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

async fn dynamic_config_poll(
    tx: Sender<HttpServerDynamicConfig>,
    config_manager_client: SharedConfigManagerClient,
    poll_interval: Duration,
) {
    let mut interval = time::interval(poll_interval);
    loop {
        interval.tick().await;
        let dynamic_config_result = config_manager_client.get_http_server_dynamic_config().await;
        // Make the config available if it was successfully updated.
        if let Ok(dynamic_config) = dynamic_config_result {
            let _ = tx.send(dynamic_config);
        }
    }
}
