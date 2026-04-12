use std::sync::Arc;
use std::time::Duration;

use apollo_batcher::bootstrap_client::{BootstrapClient, BootstrapValidation};
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use apollo_mempool_config::config::MempoolConfig;
use apollo_mempool_p2p_types::communication::SharedMempoolP2pPropagatorClient;
use apollo_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolRequest,
    MempoolResponse,
};
use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{
    CommitBlockArgs,
    MempoolResult,
    MempoolSnapshot,
    ValidationArgs,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_time::time::DefaultClock;
use async_trait::async_trait;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use starknet_api::block::{GasPrice, UnixTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, info, warn};

use crate::mempool::Mempool;
use crate::metrics::register_metrics;

pub type LocalMempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;
pub type RemoteMempoolServer = RemoteComponentServer<MempoolRequest, MempoolResponse>;

pub fn create_mempool(
    config: MempoolConfig,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    config_manager_client: SharedConfigManagerClient,
) -> MempoolCommunicationWrapper {
    let bootstrap_client = BootstrapClient::new(&config.static_config.batcher_storage_reader_url);
    let mut mempool = Mempool::new(config, Arc::new(DefaultClock));
    mempool.bootstrap_active = bootstrap_client.is_some();
    MempoolCommunicationWrapper::new(
        mempool,
        mempool_p2p_propagator_client,
        config_manager_client,
        bootstrap_client,
    )
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    config_manager_client: SharedConfigManagerClient,
    echonet_client: ClientWithMiddleware,
    bootstrap_client: Option<BootstrapClient>,
}

impl MempoolCommunicationWrapper {
    pub fn new(
        mempool: Mempool,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
        config_manager_client: SharedConfigManagerClient,
        bootstrap_client: Option<BootstrapClient>,
    ) -> Self {
        const MIN_RETRY_INTERVAL: Duration = Duration::from_millis(50);
        const MAX_RETRY_INTERVAL: Duration = Duration::from_millis(500);
        const MAX_RETRY_DURATION: Duration = Duration::from_secs(10);

        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(MIN_RETRY_INTERVAL, MAX_RETRY_INTERVAL)
            .jitter(Jitter::None)
            .build_with_total_retry_duration(MAX_RETRY_DURATION);

        let client = ClientBuilder::new(Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        MempoolCommunicationWrapper {
            mempool,
            mempool_p2p_propagator_client,
            config_manager_client,
            echonet_client: client,
            bootstrap_client,
        }
    }

    async fn send_tx_to_p2p(
        &self,
        message_metadata: Option<BroadcastedMessageMetadata>,
        tx: InternalRpcTransaction,
    ) -> MempoolResult<()> {
        match message_metadata {
            Some(message_metadata) => self
                .mempool_p2p_propagator_client
                .continue_propagation(message_metadata)
                .await
                .map_err(|_| MempoolError::P2pPropagatorClientError { tx_hash: tx.tx_hash }),
            None => {
                let tx_hash = tx.tx_hash;
                self.mempool_p2p_propagator_client
                    .add_transaction(tx)
                    .await
                    .map_err(|_| MempoolError::P2pPropagatorClientError { tx_hash })?;
                Ok(())
            }
        }
    }

    async fn update_dynamic_config(&mut self) {
        let mempool_dynamic_config = self
            .config_manager_client
            .get_mempool_dynamic_config()
            .await
            .expect("Should be able to get mempool dynamic config");
        self.mempool.update_dynamic_config(mempool_dynamic_config);
    }

    pub(crate) async fn add_tx(
        &mut self,
        args_wrapper: AddTransactionArgsWrapper,
    ) -> MempoolResult<()> {
        // Bootstrap exit polling: only query the bootstrap server while bootstrap_active is true.
        // Once the batcher reports NotInBootstrap, the flag flips permanently and we stop
        // querying, so subsequent transactions skip this block entirely. During bootstrap,
        // resource bounds validation is skipped (via should_validate_resource_bounds in Mempool).
        if self.mempool.bootstrap_active {
            if let Some(ref bootstrap_client) = self.bootstrap_client {
                match bootstrap_client.validate_bootstrap_internal_tx(&args_wrapper.args.tx).await {
                    Ok(BootstrapValidation::ValidBootstrapTx) => {
                        debug!(
                            "Processing bootstrap transaction in mempool, skipping resource bounds"
                        );
                    }
                    Ok(BootstrapValidation::NotBootstrapping) => {
                        info!(
                            "Bootstrap complete, disabling bootstrap checks in mempool permanently"
                        );
                        self.mempool.bootstrap_active = false;
                        self.bootstrap_client = None;
                    }
                    Err(e) => {
                        return Err(MempoolError::BootstrapValidationFailed { message: e });
                    }
                }
            }
        }

        if self.mempool.is_fifo() {
            let tx_hash = args_wrapper.args.tx.tx_hash();
            if !self.fetch_and_update_timestamp(tx_hash).await {
                warn!("Failed to fetch timestamp for tx {}, skipping transaction", tx_hash);
                return Ok(());
            }
        }

        self.mempool.add_tx(args_wrapper.args.clone())?;

        // TODO(AlonH): Verify that only transactions that were added to the mempool are sent.
        if let Err(p2p_client_err) =
            self.send_tx_to_p2p(args_wrapper.p2p_message_metadata, args_wrapper.args.tx).await
        {
            warn!("Failed to send transaction to P2P: {:?}", p2p_client_err);
        }

        Ok(())
    }

    fn validate_tx(&mut self, args: ValidationArgs) -> MempoolResult<()> {
        self.mempool.validate_tx(args)?;
        Ok(())
    }

    fn commit_block(&mut self, args: CommitBlockArgs) -> MempoolResult<()> {
        self.mempool.commit_block(args);
        Ok(())
    }

    fn get_txs(&mut self, n_txs: usize) -> MempoolResult<Vec<InternalRpcTransaction>> {
        self.mempool.get_txs(n_txs)
    }

    fn account_tx_in_pool_or_recent_block(
        &self,
        account_address: ContractAddress,
    ) -> MempoolResult<bool> {
        Ok(self.mempool.account_tx_in_pool_or_recent_block(account_address))
    }

    fn update_gas_price(&mut self, gas_price: GasPrice) -> MempoolResult<()> {
        self.mempool.update_gas_price(gas_price);
        Ok(())
    }

    fn mempool_snapshot(&self) -> MempoolResult<MempoolSnapshot> {
        self.mempool.mempool_snapshot()
    }

    fn get_timestamp(&mut self) -> MempoolResult<UnixTimestamp> {
        Ok(self.mempool.get_timestamp())
    }

    // Fetches timestamp from recorder and updates mempool.
    // Returns true if successful, false if failed after all retries.
    pub(crate) async fn fetch_and_update_timestamp(&mut self, tx_hash: TransactionHash) -> bool {
        // In Echonet mode we replay mainnet data. Some transactions require the original mainnet
        // block timestamp to pass. We fetch this timestamp from the recorder, which points
        // to Echonet.
        let recorder_url = &self.mempool.config.static_config.recorder_url;
        let url = match recorder_url.join(&format!("echonet/get_timestamp?tx_hash={}", tx_hash)) {
            Ok(url) => url,
            Err(e) => {
                warn!("Invalid recorder URL for tx {}: {}", tx_hash, e);
                return false;
            }
        };

        match self.try_fetch_timestamp(&url).await {
            Ok(timestamp) => {
                self.mempool.update_timestamp(tx_hash, timestamp);
                true
            }
            Err(e) => {
                warn!("Failed to fetch timestamp for tx {}: {}", tx_hash, e);
                false
            }
        }
    }

    async fn try_fetch_timestamp(&self, url: &reqwest::Url) -> Result<UnixTimestamp, String> {
        const REQUEST_TIMEOUT_SECS: u64 = 2;
        let response = self
            .echonet_client
            .get(url.as_str())
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|e| format!("request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        response.json::<UnixTimestamp>().await.map_err(|e| format!("invalid response: {}", e))
    }
}

#[async_trait]
impl ComponentRequestHandler<MempoolRequest, MempoolResponse> for MempoolCommunicationWrapper {
    async fn handle_request(&mut self, request: MempoolRequest) -> MempoolResponse {
        // Update the dynamic config before handling the request.
        self.update_dynamic_config().await;
        match request {
            MempoolRequest::ValidateTransaction(args) => {
                MempoolResponse::ValidateTransaction(self.validate_tx(args))
            }
            MempoolRequest::AddTransaction(args) => {
                MempoolResponse::AddTransaction(self.add_tx(args).await)
            }
            MempoolRequest::CommitBlock(args) => {
                MempoolResponse::CommitBlock(self.commit_block(args))
            }
            MempoolRequest::GetTransactions(n_txs) => {
                MempoolResponse::GetTransactions(self.get_txs(n_txs))
            }
            MempoolRequest::AccountTxInPoolOrRecentBlock(account_address) => {
                MempoolResponse::AccountTxInPoolOrRecentBlock(
                    self.account_tx_in_pool_or_recent_block(account_address),
                )
            }
            MempoolRequest::UpdateGasPrice(gas_price) => {
                MempoolResponse::UpdateGasPrice(self.update_gas_price(gas_price))
            }
            MempoolRequest::GetMempoolSnapshot() => {
                MempoolResponse::GetMempoolSnapshot(self.mempool_snapshot())
            }
            MempoolRequest::GetTimestamp => MempoolResponse::GetTimestamp(self.get_timestamp()),
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolCommunicationWrapper {
    async fn start(&mut self) {
        register_metrics();
    }
}
