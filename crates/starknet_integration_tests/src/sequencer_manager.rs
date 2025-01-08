use futures::future::join_all;
use futures::stream::{self, StreamExt};
use futures::TryFutureExt;
use infra_utils::run_until::run_until;
use infra_utils::tracing::{CustomLogger, TraceLevel};
use itertools::izip;
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_execution::execution_utils::get_nonce_at;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::StateNumber;
use starknet_api::transaction::TransactionHash;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::test_utils::node_runner::{spawn_run_node, NodeRunner};
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tracing::info;

use crate::integration_test_setup::SequencerSetup;
use crate::test_identifiers::TestIdentifier;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_and_channels,
    create_mempool_p2p_configs,
    send_account_txs,
};

pub type ComposedNodeComponentConfigs = Vec<ComponentConfig>;

pub struct SequencerSetupManager {
    pub sequencers: Vec<SequencerSetup>,
    pub sequencer_run_handles: Vec<JoinHandle<()>>,
}

impl SequencerSetupManager {
    pub async fn run(sequencers: Vec<SequencerSetup>) -> Self {
        info!("Running sequencers.");
        let sequencer_run_handles = sequencers
            .iter()
            .map(|sequencer_setup| {
                spawn_run_node(
                    sequencer_setup.node_config_path.clone(),
                    NodeRunner::new(
                        sequencer_setup.sequencer_index,
                        sequencer_setup.sequencer_part_index,
                    ),
                )
            })
            .collect::<Vec<_>>();

        let sequencer_manager = Self { sequencers, sequencer_run_handles };

        // Wait for the nodes to start.
        sequencer_manager.await_alive(5000, 50).await;

        sequencer_manager
    }

    async fn await_alive(&self, interval: u64, max_attempts: usize) {
        let await_alive_tasks = self.sequencers.iter().map(|sequencer| {
            let result = sequencer.monitoring_client.await_alive(interval, max_attempts);
            result.unwrap_or_else(|_| {
                panic!(
                    "Node {} part {} should be alive.",
                    sequencer.sequencer_index, sequencer.sequencer_part_index
                )
            })
        });

        join_all(await_alive_tasks).await;
    }

    pub async fn send_rpc_tx_fn(&self, rpc_tx: RpcTransaction) -> TransactionHash {
        self.sequencers[0].assert_add_tx_success(rpc_tx).await
    }

    pub fn batcher_storage_reader(&self) -> StorageReader {
        let (batcher_storage_reader, _) =
            papyrus_storage::open_storage(self.sequencers[0].batcher_storage_config.clone())
                .expect("Failed to open batcher's storage");
        batcher_storage_reader
    }

    pub fn shutdown_nodes(&self) {
        self.sequencer_run_handles.iter().for_each(|handle| {
            assert!(!handle.is_finished(), "Node should still be running.");
            handle.abort()
        });
    }

    pub async fn run_integration_test_simulator(
        &self,
        tx_generator: MultiAccountTransactionGenerator,
        n_txs: usize,
        sender_account: AccountId,
    ) {
        info!("Running integration test simulator.");
        let send_rpc_tx_fn = &mut |rpc_tx| self.send_rpc_tx_fn(rpc_tx);

        info!("Sending {n_txs} txs.");
        let tx_hashes = send_account_txs(tx_generator, sender_account, n_txs, send_rpc_tx_fn).await;
        assert_eq!(tx_hashes.len(), n_txs);
    }

    pub async fn await_execution(&self, expected_block_number: BlockNumber) {
        info!("Awaiting until {expected_block_number} blocks have been created.");
        await_block(5000, expected_block_number, 50, &self.batcher_storage_reader())
            .await
            .expect("Block number should have been reached.");
    }
}

/// Reads the latest block number from the storage.
pub fn get_latest_block_number(storage_reader: &StorageReader) -> BlockNumber {
    let txn = storage_reader.begin_ro_txn().unwrap();
    txn.get_state_marker()
        .expect("There should always be a state marker")
        .prev()
        .expect("There should be a previous block in the storage, set by the test setup")
}

/// Reads an account nonce after a block number from storage.
pub fn get_account_nonce(
    storage_reader: &StorageReader,
    contract_address: ContractAddress,
) -> Nonce {
    let block_number = get_latest_block_number(storage_reader);
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(block_number);
    get_nonce_at(&txn, state_number, None, contract_address)
        .expect("Should always be Ok(Some(Nonce))")
        .expect("Should always be Some(Nonce)")
}

/// Sample a storage until sufficiently many blocks have been stored. Returns an error if after
/// the given number of attempts the target block number has not been reached.
pub async fn await_block(
    interval: u64,
    target_block_number: BlockNumber,
    max_attempts: usize,
    storage_reader: &StorageReader,
) -> Result<BlockNumber, ()> {
    let condition = |&latest_block_number: &BlockNumber| latest_block_number >= target_block_number;
    let get_latest_block_number_closure = || async move { get_latest_block_number(storage_reader) };

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some("Waiting for storage to include block".to_string()),
    );

    run_until(interval, max_attempts, get_latest_block_number_closure, condition, Some(logger))
        .await
        .ok_or(())
}

pub async fn verify_results(
    sender_address: ContractAddress,
    batcher_storage_reader: StorageReader,
    n_txs: usize,
) {
    info!("Verifying tx sender account nonce.");
    let expected_nonce_value = n_txs + 1;
    let expected_nonce =
        Nonce(Felt::from_hex_unchecked(format!("0x{:X}", expected_nonce_value).as_str()));
    let nonce = get_account_nonce(&batcher_storage_reader, sender_address);
    assert_eq!(nonce, expected_nonce);
}

pub async fn get_sequencer_setup_configs(
    test_unique_id: TestIdentifier,
    tx_generator: &MultiAccountTransactionGenerator,
    mut available_ports: AvailablePorts,
    component_configs: Vec<ComposedNodeComponentConfigs>,
) -> Vec<SequencerSetup> {
    info!("Creating sequencer configurations.");
    let chain_info = create_chain_info();
    let accounts = tx_generator.accounts();
    let n_distributed_sequencers = component_configs
        .iter()
        .map(|composed_node_component_configs| composed_node_component_configs.len())
        .sum();

    let (consensus_manager_configs, _) = create_consensus_manager_configs_and_channels(
        n_distributed_sequencers,
        available_ports.get_next_ports(n_distributed_sequencers + 1),
    );

    let mempool_p2p_configs = create_mempool_p2p_configs(
        chain_info.chain_id.clone(),
        available_ports.get_next_ports(n_distributed_sequencers),
    );

    // Flatten while enumerating sequencer and sequencer part indices.
    let indexed_component_configs: Vec<((usize, usize), ComponentConfig)> = component_configs
        .into_iter()
        .enumerate()
        .flat_map(|(sequencer_index, parts_component_configs)| {
            parts_component_configs.into_iter().enumerate().map(
                move |(sequencer_part_config, value)| {
                    ((sequencer_index, sequencer_part_config), value) // Combine indices with the value
                },
            )
        })
        .collect();

    // TODO(Nadin/Tsabary): There are redundant p2p configs here, as each distributed node
    // needs only one of them, but the current setup creates one per part. Need to refactor.

    stream::iter(
        izip!(indexed_component_configs, consensus_manager_configs, mempool_p2p_configs)
            .enumerate(),
    )
    .then(
        |(
            index,
            (
                ((sequencer_index, sequencer_part_index), component_config),
                consensus_manager_config,
                mempool_p2p_config,
            ),
        )| {
            let value = chain_info.clone();
            async move {
                SequencerSetup::new(
                    accounts.to_vec(),
                    sequencer_index,
                    sequencer_part_index,
                    value.clone(),
                    consensus_manager_config,
                    mempool_p2p_config,
                    AvailablePorts::new(test_unique_id.into(), index.try_into().unwrap()),
                    component_config.clone(),
                )
                .await
            }
        },
    )
    .collect()
    .await
}
