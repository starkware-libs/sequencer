use std::sync::Arc;

use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

use crate::compilation::GatewayCompiler;
use crate::config::{GatewayConfig, RpcStateReaderConfig};
use crate::gateway::Gateway;
use crate::rpc_state_reader::RpcStateReaderFactory;

const N_TXS: usize = 100;

pub struct BenchTestSetupConfig {
    pub n_txs: usize,
    pub gateway_config: GatewayConfig,
    pub rpc_state_reader_config: RpcStateReaderConfig,
    pub compiler_config: SierraToCasmCompilationConfig,
}

impl Default for BenchTestSetupConfig {
    fn default() -> Self {
        Self {
            n_txs: N_TXS,
            gateway_config: GatewayConfig::default(),
            rpc_state_reader_config: RpcStateReaderConfig::default(),
            compiler_config: SierraToCasmCompilationConfig::default(),
        }
    }
}

pub struct BenchTestSetup {
    gateway: Gateway,
    txs: Vec<RpcTransaction>,
}

impl BenchTestSetup {
    pub fn new(config: BenchTestSetupConfig) -> Self {
        // TODO(Arni): Register accounts see [`register_account_for_flow_test`].
        let mut tx_generator = MultiAccountTransactionGenerator::new();
        let default_account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0);

        tx_generator.register_account(default_account);

        let mut txs: Vec<RpcTransaction> = Vec::with_capacity(config.n_txs);
        for _ in 0..config.n_txs {
            txs.push(tx_generator.account_with_id(0).
            // TODO(Arni): Do something smarter than generate raw invoke.
            generate_invoke_with_tip(1));
        }

        let state_reader_factory =
            Arc::new(RpcStateReaderFactory { config: config.rpc_state_reader_config.clone() });
        let gateway_compiler =
            GatewayCompiler::new_command_line_compiler(config.compiler_config.clone());
        let mempool_client = Arc::new(MockMempoolClient::new());
        let gateway = Gateway::new(
            config.gateway_config.clone(),
            state_reader_factory,
            gateway_compiler,
            mempool_client,
        );

        Self { gateway, txs }
    }

    pub async fn send_txs_to_gateway(&self) {
        for tx in &self.txs {
            let _tx_hash = self.gateway.add_tx(tx.clone(), None).await;
        }
    }
}
