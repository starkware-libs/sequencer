use std::sync::Arc;
use std::time::Duration;

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
    TxBlockMetadata,
    ValidationArgs,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_time::time::DefaultClock;
use async_trait::async_trait;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use starknet_api::block::{GasPrice, ReplayBlockMetadata, UnixTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::warn;

use crate::mempool::Mempool;
use crate::metrics::register_metrics;
use crate::transaction_queue_trait::BlockMetadata;

/// Response body of the recorder's `echonet/get_block_metadata` endpoint: the original block's
/// timestamp and gas prices.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RecorderBlockMetadata {
    pub timestamp: UnixTimestamp,
    pub l1_gas_price_wei: GasPrice,
    pub l1_data_gas_price_wei: GasPrice,
    pub l1_gas_price_fri: GasPrice,
    pub l1_data_gas_price_fri: GasPrice,
    pub l2_gas_price_fri: GasPrice,
}

pub type LocalMempoolServer =
    LocalComponentServer<MempoolCommunicationWrapper, MempoolRequest, MempoolResponse>;
pub type RemoteMempoolServer = RemoteComponentServer<MempoolRequest, MempoolResponse>;

pub fn create_mempool(
    config: MempoolConfig,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    config_manager_client: SharedConfigManagerClient,
) -> MempoolCommunicationWrapper {
    MempoolCommunicationWrapper::new(
        Mempool::new(config, Arc::new(DefaultClock)),
        mempool_p2p_propagator_client,
        config_manager_client,
    )
}

/// Wraps the mempool to enable inbound async communication from other components.
pub struct MempoolCommunicationWrapper {
    mempool: Mempool,
    mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
    config_manager_client: SharedConfigManagerClient,
    echonet_client: ClientWithMiddleware,
}

impl MempoolCommunicationWrapper {
    pub fn new(
        mempool: Mempool,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
        config_manager_client: SharedConfigManagerClient,
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
        if self.mempool.is_fifo() {
            let tx_hash = args_wrapper.args.tx.tx_hash();
            if !self.fetch_and_update_tx_block_metadata(tx_hash).await {
                warn!("Failed to fetch tx block metadata for tx {}, skipping transaction", tx_hash);
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

    pub(crate) async fn resolve_block_metadata(&mut self) -> MempoolResult<ReplayBlockMetadata> {
        let BlockMetadata { timestamp, block_number } = self.mempool.resolve_block_metadata();
        let fallback_metadata =
            ReplayBlockMetadata { timestamp, block_number, ..Default::default() };

        // Block numbers are only tracked in FIFO (Echonet) mode; without one there is no
        // original block to fetch metadata for.
        let Some(block_number) = block_number else {
            return Ok(fallback_metadata);
        };

        match self
            .try_fetch_json::<RecorderBlockMetadata>(&format!(
                "echonet/get_block_metadata?block_number={}",
                block_number.0
            ))
            .await
        {
            // The recorder's timestamp is authoritative over the tx-derived one; they differ
            // for empty blocks.
            Ok(RecorderBlockMetadata {
                timestamp,
                l1_gas_price_wei,
                l1_data_gas_price_wei,
                l1_gas_price_fri,
                l1_data_gas_price_fri,
                l2_gas_price_fri,
            }) => Ok(ReplayBlockMetadata {
                timestamp,
                block_number: Some(block_number),
                l1_gas_price_wei,
                l1_data_gas_price_wei,
                l1_gas_price_fri,
                l1_data_gas_price_fri,
                l2_gas_price_fri,
            }),
            Err(fetch_error) => {
                warn!("Failed to fetch block metadata for block {block_number}: {fetch_error}");
                Ok(fallback_metadata)
            }
        }
    }

    // Fetches tx block metadata from recorder and updates mempool.
    // Returns true if successful, false if failed after all retries.
    pub(crate) async fn fetch_and_update_tx_block_metadata(
        &mut self,
        tx_hash: TransactionHash,
    ) -> bool {
        // In Echonet mode we replay mainnet data. Some transactions require the original mainnet
        // metadata to pass. We fetch it from the recorder, which points to Echonet.
        match self
            .try_fetch_json::<TxBlockMetadata>(&format!(
                "echonet/get_tx_block_metadata?tx_hash={tx_hash}"
            ))
            .await
        {
            Ok(tx_block_metadata) => {
                self.mempool.update_tx_block_metadata(tx_hash, tx_block_metadata);
                true
            }
            Err(fetch_error) => {
                warn!("Failed to fetch tx block metadata for tx {tx_hash}: {fetch_error}");
                false
            }
        }
    }

    // Fetches a JSON response from the recorder endpoint at `path_and_query`.
    async fn try_fetch_json<ResponseBody: DeserializeOwned>(
        &self,
        path_and_query: &str,
    ) -> Result<ResponseBody, String> {
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
        let url = self
            .mempool
            .config
            .static_config
            .recorder_url
            .join(path_and_query)
            .map_err(|join_error| format!("invalid recorder URL: {join_error}"))?;
        let response = self
            .echonet_client
            .get(url.as_str())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|request_error| format!("request failed: {request_error}"))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        response
            .json::<ResponseBody>()
            .await
            .map_err(|parse_error| format!("invalid response: {parse_error}"))
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
            MempoolRequest::ResolveBlockMetadata => {
                MempoolResponse::ResolveBlockMetadata(self.resolve_block_metadata().await)
            }
        }
    }
}

#[async_trait]
impl ComponentStarter for MempoolCommunicationWrapper {
    async fn start(&mut self) {
        register_metrics();
    }
}
