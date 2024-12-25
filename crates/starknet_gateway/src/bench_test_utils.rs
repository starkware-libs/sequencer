use std::sync::Arc;

use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion};
use mempool_test_utils::starknet_api_test_utils::test_valid_resource_bounds;
use starknet_api::core::ContractAddress;
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::test_utils::NonceManager;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

use crate::compilation::GatewayCompiler;
use crate::config::GatewayConfig;
use crate::gateway::GatewayBusinessLogic;
use crate::state_reader_test_utils::local_test_state_reader_factory;

const N_TXS: usize = 100;

pub struct BenchTestSetupConfig {
    pub n_txs: usize,
    pub gateway_config: GatewayConfig,
    pub compiler_config: SierraToCasmCompilationConfig,
}

impl Default for BenchTestSetupConfig {
    fn default() -> Self {
        Self {
            n_txs: N_TXS,
            gateway_config: GatewayConfig {
                chain_info: ChainInfo::create_for_testing(),
                ..Default::default()
            },
            compiler_config: SierraToCasmCompilationConfig::default(),
        }
    }
}

pub struct BenchTestSetup {
    gateway: GatewayBusinessLogic,
    txs: Vec<RpcTransaction>,
}

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

impl BenchTestSetup {
    pub fn new(config: BenchTestSetupConfig) -> Self {
        let cairo_version = CairoVersion::Cairo0;
        let mut tx_generator = TransactionGenerator::new(cairo_version);

        let mut txs: Vec<RpcTransaction> = Vec::with_capacity(config.n_txs);
        for _ in 0..config.n_txs {
            txs.push(tx_generator.
            // TODO(Arni): Do something smarter than generate raw invoke.
            generate_invoke());
        }

        let state_reader_factory = local_test_state_reader_factory(cairo_version, false);
        let gateway_compiler =
            GatewayCompiler::new_command_line_compiler(config.compiler_config.clone());

        let gateway_business_logic = GatewayBusinessLogic::new(
            config.gateway_config,
            Arc::new(state_reader_factory),
            gateway_compiler,
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
