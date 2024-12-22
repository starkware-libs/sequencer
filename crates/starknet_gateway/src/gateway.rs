use std::clone::Clone;
use std::sync::Arc;

use blockifier::context::ChainInfo;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::{AddTransactionArgsWrapper, SharedMempoolClient};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use tracing::{error, info, instrument, Span};

use crate::compilation::GatewayCompiler;
use crate::config::{GatewayConfig, RpcStateReaderConfig};
use crate::errors::GatewayResult;
use crate::rpc_state_reader::RpcStateReaderFactory;
use crate::state_reader::StateReaderFactory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::compile_contract_and_build_executable_tx;

#[cfg(test)]
#[path = "gateway_test.rs"]
pub mod gateway_test;

pub struct Gateway {
    pub config: GatewayConfig,
    pub business_logic: GatewayBusinessLogic,
    pub mempool_client: SharedMempoolClient,
}

impl Gateway {
    pub fn new(
        config: GatewayConfig,
        state_reader_factory: Arc<dyn StateReaderFactory>,
        gateway_compiler: GatewayCompiler,
        mempool_client: SharedMempoolClient,
    ) -> Self {
        let business_logic = GatewayBusinessLogic {
            stateless_tx_validator: Arc::new(StatelessTransactionValidator {
                config: config.stateless_tx_validator_config.clone(),
            }),
            stateful_tx_validator: Arc::new(StatefulTransactionValidator {
                config: config.stateful_tx_validator_config.clone(),
            }),
            state_reader_factory,
            gateway_compiler: Arc::new(gateway_compiler),
            chain_info: config.chain_info.clone(),
        };
        Self { config: config.clone(), business_logic, mempool_client }
    }

    #[instrument(skip(self), ret)]
    pub async fn add_tx(
        &self,
        tx: RpcTransaction,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    ) -> GatewayResult<TransactionHash> {
        info!("Processing tx");
        let gateway_business_logic = self.business_logic.clone();
        // Run the blocking task in the current span.
        let curr_span = Span::current();
        let add_tx_args = tokio::task::spawn_blocking(move || {
            curr_span.in_scope(|| gateway_business_logic.process_tx(tx))
        })
        .await
        .map_err(|join_err| {
            error!("Failed to process tx: {}", join_err);
            GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
        })??;

        let tx_hash = add_tx_args.tx.tx_hash();

        let add_tx_args = AddTransactionArgsWrapper { args: add_tx_args, p2p_message_metadata };
        self.mempool_client.add_tx(add_tx_args).await.map_err(|e| {
            error!("Failed to send tx to mempool: {}", e);
            GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
        })?;
        // TODO: Also return `ContractAddress` for deploy and `ClassHash` for Declare.
        Ok(tx_hash)
    }
}

#[derive(Clone)]
pub struct GatewayBusinessLogic {
    pub stateless_tx_validator: Arc<StatelessTransactionValidator>,
    pub stateful_tx_validator: Arc<StatefulTransactionValidator>,
    pub state_reader_factory: Arc<dyn StateReaderFactory>,
    pub gateway_compiler: Arc<GatewayCompiler>,
    pub chain_info: ChainInfo,
}

impl GatewayBusinessLogic {
    /// CPU-intensive transaction processing, spawned in a blocking thread to avoid blocking other
    /// tasks from running.
    fn process_tx(&self, tx: RpcTransaction) -> GatewayResult<AddTransactionArgs> {
        // TODO(Arni, 1/5/2024): Perform congestion control.

        // Perform stateless validations.
        self.stateless_tx_validator.validate(&tx)?;

        let executable_tx = compile_contract_and_build_executable_tx(
            tx,
            self.gateway_compiler.as_ref(),
            &self.chain_info.chain_id,
        )?;

        // Perform post compilation validations.
        if let AccountTransaction::Declare(executable_declare_tx) = &executable_tx {
            if !executable_declare_tx.validate_compiled_class_hash() {
                return Err(GatewaySpecError::CompiledClassHashMismatch);
            }
        }

        let mut validator = self
            .stateful_tx_validator
            .instantiate_validator(self.state_reader_factory.as_ref(), &self.chain_info)?;
        let address = executable_tx.contract_address();
        let nonce = validator.get_nonce(address).map_err(|e| {
            error!("Failed to get nonce for sender address {}: {}", address, e);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })?;

        self.stateful_tx_validator.run_validate(&executable_tx, nonce, validator)?;

        // TODO(Arni): Add the Sierra and the Casm to the mempool input.
        Ok(AddTransactionArgs { tx: executable_tx, account_state: AccountState { address, nonce } })
    }
}

pub fn create_gateway(
    config: GatewayConfig,
    rpc_state_reader_config: RpcStateReaderConfig,
    compiler_config: SierraToCasmCompilationConfig,
    mempool_client: SharedMempoolClient,
) -> Gateway {
    let state_reader_factory = Arc::new(RpcStateReaderFactory { config: rpc_state_reader_config });
    let gateway_compiler = GatewayCompiler::new_command_line_compiler(compiler_config);

    Gateway::new(config, state_reader_factory, gateway_compiler, mempool_client)
}

impl ComponentStarter for Gateway {}
