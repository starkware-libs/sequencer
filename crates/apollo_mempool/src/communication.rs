use std::collections::HashMap;
use std::sync::Arc;

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_deployment_mode::DeploymentMode;
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
use starknet_api::block::{GasPrice, UnixTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;
use tokio::time::sleep;
use tracing::{debug, warn};

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
    http_client: Client,
}

impl MempoolCommunicationWrapper {
    pub fn new(
        mempool: Mempool,
        mempool_p2p_propagator_client: SharedMempoolP2pPropagatorClient,
        config_manager_client: SharedConfigManagerClient,
    ) -> Self {
        MempoolCommunicationWrapper {
            mempool,
            mempool_p2p_propagator_client,
            config_manager_client,
            http_client: Client::new(),
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
        if self.mempool.config.static_config.deployment_mode == DeploymentMode::Echonet {
            let tx_hash = args_wrapper.args.tx.tx_hash();
            self.fetch_and_update_timestamp(tx_hash).await;
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

    fn update_timestamps(
        &mut self,
        mappings: HashMap<TransactionHash, UnixTimestamp>,
    ) -> MempoolResult<()> {
        self.mempool.update_timestamps(mappings);
        Ok(())
    }

    async fn fetch_and_update_timestamp(&mut self, tx_hash: TransactionHash) {
        let recorder_url = &self.mempool.config.static_config.recorder_url;
        const MAX_RETRIES: usize = 2;
        const RETRY_DELAY_MS: u64 = 50;

        for attempt in 0..MAX_RETRIES {
            let url = format!("{}/echonet/get_timestamp?tx_hash={}", recorder_url, tx_hash);

            let response = match self
                .http_client
                .get(&url)
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => res,
                Ok(res) => {
                    debug!(
                        "HTTP error from recorder for tx {} (attempt {}): {}",
                        tx_hash,
                        attempt + 1,
                        res.status()
                    );
                    if attempt + 1 < MAX_RETRIES {
                        sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                        continue;
                    }
                    warn!(
                        "Failed to fetch timestamp for tx {} after {} attempts",
                        tx_hash, MAX_RETRIES
                    );
                    return;
                }
                Err(e) => {
                    debug!(
                        "Failed to fetch timestamp for tx {} (attempt {}): {}",
                        tx_hash,
                        attempt + 1,
                        e
                    );
                    if attempt + 1 < MAX_RETRIES {
                        sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                        continue;
                    }
                    warn!(
                        "Failed to fetch timestamp for tx {} after {} attempts",
                        tx_hash, MAX_RETRIES
                    );
                    return;
                }
            };

            match response.json::<UnixTimestamp>().await {
                Ok(timestamp) => {
                    debug!("Fetched timestamp {} for tx {}", timestamp, tx_hash);
                    let mut mappings = HashMap::new();
                    mappings.insert(tx_hash, timestamp);
                    self.mempool.update_timestamps(mappings);
                    return;
                }
                Err(e) => {
                    debug!(
                        "Failed to parse timestamp response for tx {} (attempt {}): {}",
                        tx_hash,
                        attempt + 1,
                        e
                    );
                    if attempt + 1 < MAX_RETRIES {
                        sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                        continue;
                    }
                    warn!(
                        "Failed to fetch timestamp for tx {} after {} attempts",
                        tx_hash, MAX_RETRIES
                    );
                    return;
                }
            }
        }
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
            MempoolRequest::UpdateTimestamps(mappings) => {
                MempoolResponse::UpdateTimestamps(self.update_timestamps(mappings))
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
