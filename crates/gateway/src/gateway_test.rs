use std::sync::Arc;

use assert_matches::assert_matches;
use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::context::ChainInfo;
use blockifier::execution::contract_class::ContractClass;
use blockifier::test_utils::CairoVersion;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use rstest::{fixture, rstest};
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::{RPCDeclareTransaction, RPCTransaction};
use starknet_api::transaction::TransactionHash;
use starknet_mempool::communication::create_mempool_server;
use starknet_mempool::mempool::Mempool;
use starknet_mempool_types::communication::{MempoolClientImpl, MempoolRequestAndResponseSender};
use starknet_sierra_compile::errors::CompilationUtilError;
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::errors::GatewayError;
use crate::gateway::{add_tx, compile_contract_class, AppState, SharedMempoolClient};
use crate::starknet_api_test_utils::{declare_tx, deploy_account_tx, invoke_tx};
use crate::state_reader_test_utils::{
    local_test_state_reader_factory, local_test_state_reader_factory_for_deploy_account,
    TestStateReaderFactory,
};
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::{external_tx_to_account_tx, get_tx_hash};

const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

pub fn app_state(
    mempool_client: SharedMempoolClient,
    state_reader_factory: TestStateReaderFactory,
) -> AppState {
    AppState {
        stateless_tx_validator: StatelessTransactionValidator {
            config: StatelessTransactionValidatorConfig {
                validate_non_zero_l1_gas_fee: true,
                max_calldata_length: 10,
                max_signature_length: 2,
                max_bytecode_size: 10000,
                max_raw_class_size: 1000000,
                ..Default::default()
            },
        },
        stateful_tx_validator: Arc::new(StatefulTransactionValidator {
            config: StatefulTransactionValidatorConfig::create_for_testing(),
        }),
        state_reader_factory: Arc::new(state_reader_factory),
        mempool_client,
    }
}

// TODO(Ayelet): add test cases for declare.
#[tokio::test]
#[rstest]
#[case::valid_invoke_tx_cairo1(
    invoke_tx(CairoVersion::Cairo1),
    local_test_state_reader_factory(CairoVersion::Cairo1, false)
)]
#[case::valid_invoke_tx_cairo0(
    invoke_tx(CairoVersion::Cairo0),
    local_test_state_reader_factory(CairoVersion::Cairo0, false)
)]
#[case::valid_deploy_account_tx(
    deploy_account_tx(),
    local_test_state_reader_factory_for_deploy_account(&tx)
)]
#[case::valid_declare_tx(
    declare_tx(),
    local_test_state_reader_factory(CairoVersion::Cairo1, false)
)]
async fn test_add_tx(
    #[case] tx: RPCTransaction,
    #[case] state_reader_factory: TestStateReaderFactory,
    mempool: Mempool,
) {
    // TODO(Tsabary): wrap creation of channels in dedicated functions, take channel capacity from
    // config.
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    let mut mempool_server = create_mempool_server(mempool, rx_mempool);
    task::spawn(async move {
        mempool_server.start().await;
    });

    let mempool_client = Arc::new(MempoolClientImpl::new(tx_mempool));

    let app_state = app_state(mempool_client, state_reader_factory);

    let tx_hash = calculate_hash(&tx);
    let response = add_tx(State(app_state), tx.into()).await.into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    assert_eq!(tx_hash, serde_json::from_slice(response_bytes).unwrap());
}

#[test]
fn test_compile_contract_class_compiled_class_hash_missmatch() {
    let mut tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => tx
    );
    let expected_hash_result = tx.compiled_class_hash;
    let supplied_hash = CompiledClassHash::default();

    tx.compiled_class_hash = supplied_hash;
    let declare_tx = RPCDeclareTransaction::V3(tx);

    let result = compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompiledClassHashMismatch { supplied, hash_result }
        if supplied == supplied_hash && hash_result == expected_hash_result
    );
}

#[test]
fn test_compile_contract_class_bad_sierra() {
    let mut tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => tx
    );
    // Truncate the sierra program to trigger an error.
    tx.contract_class.sierra_program = tx.contract_class.sierra_program[..100].to_vec();
    let declare_tx = RPCDeclareTransaction::V3(tx);

    let result = compile_contract_class(&declare_tx);
    assert_matches!(
        result.unwrap_err(),
        GatewayError::CompilationError(CompilationUtilError::AllowedLibfuncsError(
            AllowedLibfuncsError::SierraProgramError
        ))
    )
}

#[test]
fn test_compile_contract_class() {
    let declare_tx = assert_matches!(
        declare_tx(),
        RPCTransaction::Declare(declare_tx) => declare_tx
    );
    let RPCDeclareTransaction::V3(declare_tx_v3) = &declare_tx;
    let contract_class = &declare_tx_v3.contract_class;

    let class_info = compile_contract_class(&declare_tx).unwrap();
    assert_matches!(class_info.contract_class(), ContractClass::V1(_));
    assert_eq!(class_info.sierra_program_length(), contract_class.sierra_program.len());
    assert_eq!(class_info.abi_length(), contract_class.abi.len());
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

fn calculate_hash(external_tx: &RPCTransaction) -> TransactionHash {
    let optional_class_info = match &external_tx {
        RPCTransaction::Declare(declare_tx) => Some(compile_contract_class(declare_tx).unwrap()),
        _ => None,
    };

    let account_tx = external_tx_to_account_tx(
        external_tx,
        optional_class_info,
        &ChainInfo::create_for_testing().chain_id,
    )
    .unwrap();
    get_tx_hash(&account_tx)
}
