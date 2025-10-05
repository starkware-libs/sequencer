use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::TransactionConverter;
use apollo_class_manager_types::EmptyClassManagerClient;
use apollo_gateway::gateway::Gateway;
use apollo_gateway::state_reader_test_utils::local_test_state_reader_factory;
use apollo_gateway::stateless_transaction_validator::StatelessTransactionValidator;
use apollo_gateway_config::config::GatewayConfig;
use apollo_mempool_types::communication::MockMempoolClient;
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_trivial_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::test_valid_resource_bounds;
use starknet_api::core::ContractAddress;
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::test_utils::NonceManager;

const N_TXS: usize = 100;

// TODO(Arni): Use `AccountTransactionGenerator` from `starknet_api_test_utils`.
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

    fn generate_invoke(&mut self) -> RpcTransaction {
        let invoke_args = invoke_tx_args!(
            nonce: self.nonce_manager.next(self.sender_address),
            sender_address: self.sender_address,
            resource_bounds: test_valid_resource_bounds(),
            calldata: create_trivial_calldata(self.test_contract_address),
        );
        rpc_invoke_tx(invoke_args)
    }
}

pub struct BenchTestSetupConfig {
    pub n_txs: usize,
    pub gateway_config: GatewayConfig,
}

impl Default for BenchTestSetupConfig {
    fn default() -> Self {
        Self {
            n_txs: N_TXS,
            gateway_config: GatewayConfig {
                chain_info: ChainInfo::create_for_testing(),
                ..Default::default()
            },
        }
    }
}

pub struct BenchTestSetup {
    gateway: Gateway,
    txs: Vec<RpcTransaction>,
}

impl BenchTestSetup {
    pub fn new(config: BenchTestSetupConfig) -> Self {
        let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
        let mut tx_generator = TransactionGenerator::new(cairo_version);

        let mut txs: Vec<RpcTransaction> = Vec::with_capacity(config.n_txs);
        for _ in 0..config.n_txs {
            txs.push(tx_generator.
            // TODO(Arni): Do something smarter than generate raw invoke.
            generate_invoke());
        }

        let state_reader_factory = local_test_state_reader_factory(cairo_version, false);
        let mut mempool_client = MockMempoolClient::new();
        let class_manager_client = Arc::new(EmptyClassManagerClient);
        let transaction_converter = TransactionConverter::new(
            class_manager_client.clone(),
            config.gateway_config.chain_info.chain_id.clone(),
        );
        let stateless_tx_validator = Arc::new(StatelessTransactionValidator {
            config: config.gateway_config.stateless_tx_validator_config.clone(),
        });
        mempool_client.expect_add_tx().returning(|_| Ok(()));

        let gateway_business_logic = Gateway::new(
            config.gateway_config,
            Arc::new(state_reader_factory),
            Arc::new(mempool_client),
            Arc::new(transaction_converter),
            stateless_tx_validator,
        );

        Self { gateway: gateway_business_logic, txs }
    }

    pub async fn send_txs_to_gateway(&self) {
        for tx in &self.txs {
            let _tx_hash = self
                .gateway
                .add_tx(tx.clone(), None)
                .await
                .expect("Some txs has failed in the gateway.");
        }
    }
}
