use std::sync::Arc;

use assert_matches::assert_matches;
use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::{declare_tx, invoke_tx};
use mockall::predicate::eq;
use starknet_api::core::{ChainId, CompiledClassHash, ContractAddress};
use starknet_api::executable_transaction::{InvokeTransaction, Transaction};
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_mempool_types::mempool_types::{Account, AccountState, MempoolInput};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;

use crate::compilation::GatewayCompiler;
use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::gateway::{add_tx, AppState, SharedMempoolClient};
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
            config: StatefulTransactionValidatorConfig::create_for_testing(),
        }),
        gateway_compiler: GatewayCompiler::new_command_line_compiler(
            SierraToCasmCompilationConfig::default(),
        ),
        state_reader_factory: Arc::new(state_reader_factory),
        mempool_client,
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

#[tokio::test]
async fn test_add_tx() {
    let (rpc_tx, sender_address) = create_tx();
    let rpc_invoke_tx =
        assert_matches!(rpc_tx.clone(), RpcTransaction::Invoke(rpc_invoke_tx) => rpc_invoke_tx);
    let executable_tx = Transaction::Invoke(
        InvokeTransaction::from_rpc_tx(rpc_invoke_tx, &ChainId::create_for_testing()).unwrap(),
    );

    let tx_hash = executable_tx.tx_hash();

    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_add_tx()
        .once()
        .with(eq(MempoolInput {
            tx: executable_tx,
            account: Account { sender_address, state: AccountState { nonce: *rpc_tx.nonce() } },
        }))
        .return_once(|_| Ok(()));
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let app_state = app_state(Arc::new(mock_mempool_client), state_reader_factory);

    let response = add_tx(State(app_state), rpc_tx.into()).await.into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    assert_eq!(tx_hash, serde_json::from_slice(response_bytes).unwrap());
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

// Gateway spec errors tests.
// TODO(Arni): Add tests for all the error cases. Check the response (use `into_response` on the
// result of `add_tx`).

#[tokio::test]
async fn test_compiled_class_hash_mismatch() {
    let mut declare_tx =
        assert_matches!(declare_tx(), RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);
    declare_tx.compiled_class_hash = CompiledClassHash::default();
    let tx = RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx));

    let mock_mempool_client = MockMempoolClient::new();
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let app_state = app_state(Arc::new(mock_mempool_client), state_reader_factory);

    let err = add_tx(State(app_state), tx.into()).await.unwrap_err();
    assert_matches!(err, GatewaySpecError::CompiledClassHashMismatch);
}
