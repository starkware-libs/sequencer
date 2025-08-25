use std::sync::Arc;

use apollo_mempool::config::MempoolConfig;
use apollo_mempool::mempool::Mempool;
use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use apollo_time::time::DefaultClock;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_trivial_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::test_valid_resource_bounds;
use starknet_api::core::ContractAddress;
use starknet_api::test_utils::invoke::internal_invoke_tx;
use starknet_api::test_utils::NonceManager;
use starknet_api::{invoke_tx_args, tx_hash};

const N_TXS: usize = 100;

struct TransactionGenerator {
    nonce_manager: NonceManager,
    sender_address: ContractAddress,
    test_contract_address: ContractAddress,
}

impl TransactionGenerator {
    fn new(cairo_version: CairoVersion) -> Self {
        let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
        let test_contract = FeatureContract::TestContract(cairo_version);
        let sender_address = account_contract.get_instance_address(0);
        let test_contract_address = test_contract.get_instance_address(0);
        Self { nonce_manager: NonceManager::default(), sender_address, test_contract_address }
    }

    fn generate_invoke(&mut self, index: usize) -> AddTransactionArgs {
        let nonce = self.nonce_manager.next(self.sender_address);
        let invoke_args = invoke_tx_args!(
            nonce: nonce,
            sender_address: self.sender_address,
            resource_bounds: test_valid_resource_bounds(),
            calldata: create_trivial_calldata(self.test_contract_address),
            tx_hash: tx_hash!(index + 100), // Use index to create a unique hash
        );
        let internal_rpc_invoke_tx = internal_invoke_tx(invoke_args);

        AddTransactionArgs {
            tx: internal_rpc_invoke_tx,
            account_state: AccountState { address: self.sender_address, nonce },
        }
    }
}

#[derive(Clone)]
pub struct BenchTestSetupConfig {
    pub n_txs: usize,
    pub mempool_config: MempoolConfig,
    pub add_to_get_ratio: usize, // Number of "add_tx" requests per one "get_tx" request.
}

impl Default for BenchTestSetupConfig {
    fn default() -> Self {
        Self { n_txs: N_TXS, mempool_config: MempoolConfig::default(), add_to_get_ratio: 1 }
    }
}

pub struct BenchTestSetup {
    config: BenchTestSetupConfig,
    txs: Vec<AddTransactionArgs>,
}

impl BenchTestSetup {
    pub fn new(config: &BenchTestSetupConfig) -> Self {
        let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
        let mut tx_generator = TransactionGenerator::new(cairo_version);

        let mut txs: Vec<AddTransactionArgs> = Vec::with_capacity(config.n_txs);
        txs.extend((0..config.n_txs).map(|i| tx_generator.generate_invoke(i)));

        Self { config: config.clone(), txs }
    }

    pub async fn mempool_add_get_txs(&self) {
        let mut mempool = Mempool::new(self.config.mempool_config.clone(), Arc::new(DefaultClock));

        let mut add_txs_downcount = self.config.add_to_get_ratio;
        for tx in &self.txs {
            mempool
                .add_tx(tx.clone())
                .unwrap_or_else(|e| panic!("Failed to add tx to mempool: {e:?}"));
            add_txs_downcount -= 1;
            if add_txs_downcount == 0 {
                add_txs_downcount = self.config.add_to_get_ratio;
                // Every `add_to_get_ratio` txs, we also get some txs from the mempool.
                let _ = mempool.get_txs(add_txs_downcount);
            }
        }
    }
}
