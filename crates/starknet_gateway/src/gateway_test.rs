use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::RunnableContractVersion;
use mempool_test_utils::starknet_api_test_utils::{declare_tx, invoke_tx};
use mockall::predicate::eq;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rstest::{fixture, rstest};
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress};
use starknet_api::executable_transaction::{AccountTransaction, InvokeTransaction};
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::{AddTransactionArgsWrapper, MockMempoolClient};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

use crate::compilation::GatewayCompiler;
use crate::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use crate::gateway::Gateway;
use crate::state_reader_test_utils::{local_test_state_reader_factory, TestStateReaderFactory};

#[fixture]
fn config() -> GatewayConfig {
    GatewayConfig {
        stateless_tx_validator_config: StatelessTransactionValidatorConfig::default(),
        stateful_tx_validator_config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
    }
}

#[fixture]
fn compiler() -> GatewayCompiler {
    GatewayCompiler::new_command_line_compiler(SierraToCasmCompilationConfig::default())
}

#[fixture]
fn state_reader_factory() -> TestStateReaderFactory {
    local_test_state_reader_factory(RunnableContractVersion::Cairo1Casm, false)
}

#[fixture]
fn mock_dependencies(
    config: GatewayConfig,
    compiler: GatewayCompiler,
    state_reader_factory: TestStateReaderFactory,
) -> MockDependencies {
    let mock_mempool_client = MockMempoolClient::new();
    MockDependencies { config, compiler, state_reader_factory, mock_mempool_client }
}

struct MockDependencies {
    config: GatewayConfig,
    compiler: GatewayCompiler,
    state_reader_factory: TestStateReaderFactory,
    mock_mempool_client: MockMempoolClient,
}

impl MockDependencies {
    fn gateway(self) -> Gateway {
        Gateway::new(
            self.config,
            Arc::new(self.state_reader_factory),
            self.compiler,
            Arc::new(self.mock_mempool_client),
        )
    }

    fn expect_add_tx(&mut self, args: AddTransactionArgsWrapper) {
        self.mock_mempool_client.expect_add_tx().once().with(eq(args)).return_once(|_| Ok(()));
    }
}

type SenderAddress = ContractAddress;

fn create_tx() -> (RpcTransaction, SenderAddress) {
    let tx = invoke_tx(RunnableContractVersion::Cairo1Casm);
    let sender_address = match &tx {
        RpcTransaction::Invoke(starknet_api::rpc_transaction::RpcInvokeTransaction::V3(
            invoke_tx,
        )) => invoke_tx.sender_address,
        _ => panic!("Unexpected transaction type"),
    };
    (tx, sender_address)
}

// TODO: add test with Some broadcasted message metadata
#[rstest]
#[tokio::test]
async fn test_add_tx(mut mock_dependencies: MockDependencies) {
    let (rpc_tx, address) = create_tx();
    let rpc_invoke_tx =
        assert_matches!(rpc_tx.clone(), RpcTransaction::Invoke(rpc_invoke_tx) => rpc_invoke_tx);
    let executable_tx = AccountTransaction::Invoke(
        InvokeTransaction::from_rpc_tx(rpc_invoke_tx, &ChainId::create_for_testing()).unwrap(),
    );

    let tx_hash = executable_tx.tx_hash();

    let p2p_message_metadata = Some(BroadcastedMessageMetadata::get_test_instance(&mut get_rng()));
    let add_tx_args = AddTransactionArgs {
        tx: executable_tx,
        account_state: AccountState { address, nonce: *rpc_tx.nonce() },
    };
    mock_dependencies.expect_add_tx(AddTransactionArgsWrapper {
        args: add_tx_args,
        p2p_message_metadata: p2p_message_metadata.clone(),
    });

    let gateway = mock_dependencies.gateway();

    let response_tx_hash = gateway.add_tx(rpc_tx, p2p_message_metadata).await.unwrap();

    assert_eq!(tx_hash, response_tx_hash);
}

// Gateway spec errors tests.
// TODO(Arni): Add tests for all the error cases. Check the response (use `into_response` on the
// result of `add_tx`).
// TODO(shahak): Test that when an error occurs in handle_request, then it returns the given p2p
// metadata.

#[rstest]
#[tokio::test]
async fn test_compiled_class_hash_mismatch(mock_dependencies: MockDependencies) {
    let mut declare_tx =
        assert_matches!(declare_tx(), RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);
    declare_tx.compiled_class_hash = CompiledClassHash::default();
    let tx = RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx));

    let gateway = mock_dependencies.gateway();

    let err = gateway.add_tx(tx, None).await.unwrap_err();
    assert_matches!(err, GatewaySpecError::CompiledClassHashMismatch);
}
