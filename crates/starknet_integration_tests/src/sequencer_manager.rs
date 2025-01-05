use infra_utils::run_until::run_until;
use infra_utils::tracing::{CustomLogger, TraceLevel};
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
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
use tokio::task::JoinHandle;
use tracing::info;

use crate::integration_test_setup::IntegrationSequencerSetup;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_and_channels,
    create_mempool_p2p_configs,
};

pub struct SequencerManager {
    pub sequencers: Vec<IntegrationSequencerSetup>,
    pub sequencer_run_handles: Vec<JoinHandle<()>>,
}

impl SequencerManager {
    pub async fn run(
        tx_generator: &MultiAccountTransactionGenerator,
        mut available_ports: AvailablePorts,
        component_configs: Vec<Vec<ComponentConfig>>,
    ) -> Self {
        let chain_info = create_chain_info();
        let accounts = tx_generator.accounts();
        let n_distributed_sequencers =
            component_configs.iter().map(|inner_vec| inner_vec.len()).sum();

        let (mut consensus_manager_configs, _) = create_consensus_manager_configs_and_channels(
            n_distributed_sequencers,
            &mut available_ports,
        );

        let ports = available_ports.get_next_ports(n_distributed_sequencers);
        let mut mempool_p2p_configs =
            create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);

        let mut sequencers = vec![];
        for (sequencer_id, node_composition) in component_configs.iter().enumerate() {
            for component_config in node_composition {
                // Declare one consensus_manager_config and one mempool_p2p_config for each node
                // composition
                let consensus_manager_config = consensus_manager_configs.remove(0);
                let mempool_p2p_config = mempool_p2p_configs.remove(0);
                let sequencer = IntegrationSequencerSetup::new(
                    accounts.to_vec(),
                    sequencer_id,
                    chain_info.clone(),
                    consensus_manager_config,
                    mempool_p2p_config,
                    &mut available_ports,
                    component_config.clone(),
                )
                .await;
                sequencers.push(sequencer);
            }
        }

        info!("Running sequencers.");
        let sequencer_run_handles = sequencers
            .iter()
            .enumerate()
            .map(|(i, sequencer)| {
                spawn_run_node(sequencer.node_config_path.clone(), NodeRunner::new(i))
            })
            .collect::<Vec<_>>();

        Self { sequencers, sequencer_run_handles }
    }

    pub async fn await_alive(&self, interval: u64, max_attempts: usize) {
        for (sequencer_index, sequencer) in self.sequencers.iter().enumerate() {
            sequencer
                .monitoring_client
                .await_alive(interval, max_attempts)
                .await
                .unwrap_or_else(|_| panic!("Node {} should be alive.", sequencer_index));
        }
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
