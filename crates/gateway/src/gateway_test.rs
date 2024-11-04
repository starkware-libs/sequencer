use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::context::ChainInfo;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::{declare_tx, invoke_tx};
use mockall::predicate::eq;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress};
use starknet_api::executable_transaction::{AccountTransaction, InvokeTransaction};
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::{AddTransactionArgsWrapper, MockMempoolClient};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

use crate::compilation::GatewayCompiler;
use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::gateway::{internal_add_tx, AppState, SharedMempoolClient};
use crate::state_reader_test_utils::{local_test_state_reader_factory, TestStateReaderFactory};
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;

pub fn app_state(
    mempool_client: SharedMempoolClient,
    state_reader_factory: TestStateReaderFactory,
) -> AppState {
    AppState {
        stateless_tx_validator: StatelessTransactionValidator {
            config: StatelessTransactionValidatorConfig::default(),
        },
        stateful_tx_validator: Arc::new(StatefulTransactionValidator {
            config: StatefulTransactionValidatorConfig::default(),
        }),
        gateway_compiler: GatewayCompiler::new_command_line_compiler(
            SierraToCasmCompilationConfig::default(),
        ),
        state_reader_factory: Arc::new(state_reader_factory),
        mempool_client,
        chain_info: ChainInfo::create_for_testing(),
    }
}

type SenderAddress = ContractAddress;

fn create_tx() -> (RpcTransaction, SenderAddress) {
    let tx = invoke_tx(CairoVersion::Cairo1);
    let sender_address = match &tx {
        RpcTransaction::Invoke(starknet_api::rpc_transaction::RpcInvokeTransaction::V3(
            invoke_tx,
        )) => invoke_tx.sender_address,
        _ => panic!("Unexpected transaction type"),
    };
    (tx, sender_address)
}

// TODO: add test with Some broadcasted message metadata
#[tokio::test]
async fn test_add_tx() {
    let (rpc_tx, address) = create_tx();
    let rpc_invoke_tx =
        assert_matches!(rpc_tx.clone(), RpcTransaction::Invoke(rpc_invoke_tx) => rpc_invoke_tx);
    let executable_tx = AccountTransaction::Invoke(
        InvokeTransaction::from_rpc_tx(rpc_invoke_tx, &ChainId::create_for_testing()).unwrap(),
    );

    let tx_hash = executable_tx.tx_hash();

    let mut mock_mempool_client = MockMempoolClient::new();
    let p2p_message_metadata = Some(BroadcastedMessageMetadata::get_test_instance(&mut get_rng()));
    let add_tx_args = AddTransactionArgs {
        tx: executable_tx,
        account_state: AccountState { address, nonce: *rpc_tx.nonce() },
    };
    mock_mempool_client
        .expect_add_tx()
        .once()
        .with(eq(AddTransactionArgsWrapper {
            args: add_tx_args,
            p2p_message_metadata: p2p_message_metadata.clone(),
        }))
        .return_once(|_| Ok(()));
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let app_state = app_state(Arc::new(mock_mempool_client), state_reader_factory);

    let response_tx_hash = internal_add_tx(app_state, rpc_tx, p2p_message_metadata).await.unwrap();

    assert_eq!(tx_hash, response_tx_hash);
}

// Gateway spec errors tests.
// TODO(Arni): Add tests for all the error cases. Check the response (use `into_response` on the
// result of `add_tx`).
// TODO(shahak): Test that when an error occurs in handle_request, then it returns the given p2p
// metadata.

#[tokio::test]
async fn test_compiled_class_hash_mismatch() {
    let mut declare_tx =
        assert_matches!(declare_tx(), RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);
    declare_tx.compiled_class_hash = CompiledClassHash::default();
    let tx = RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx));

    let mock_mempool_client = MockMempoolClient::new();
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let app_state = app_state(Arc::new(mock_mempool_client), state_reader_factory);

    let err = internal_add_tx(app_state, tx, None).await.unwrap_err();
    assert_matches!(err, GatewaySpecError::CompiledClassHashMismatch);
}
