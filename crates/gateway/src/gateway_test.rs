use std::sync::Arc;

use assert_matches::assert_matches;
use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::context::ChainInfo;
use blockifier::execution::contract_class::ClassInfo;
use blockifier::test_utils::CairoVersion;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transactions::{
    DeclareTransaction as BlockifierDeclareTransaction,
    DeployAccountTransaction as BlockifierDeployAccountTransaction,
    InvokeTransaction as BlockifierInvokeTransaction,
};
use mempool_test_utils::starknet_api_test_utils::{create_executable_tx, declare_tx, invoke_tx};
use mockall::predicate::eq;
use starknet_api::core::{
    calculate_contract_address,
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
};
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    TransactionHash,
    TransactionHasher,
    ValidResourceBounds,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::MockMempoolClient;
use starknet_mempool_types::mempool_types::{Account, AccountState, MempoolInput};
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use tracing::error;

use crate::compilation::GatewayCompiler;
use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::errors::StatefulTransactionValidatorResult;
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
    let (tx, sender_address) = create_tx();
    let tx_hash = calculate_hash(&tx);

    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_add_tx()
        .once()
        .with(eq(MempoolInput {
            // TODO(Arni): Use external_to_executable_tx instead of `create_executable_tx`. Consider
            // creating a `convertor for testing` that does not do the compilation.
            tx: create_executable_tx(
                sender_address,
                tx_hash,
                *tx.tip(),
                *tx.nonce(),
                ValidResourceBounds::AllResources(tx.resource_bounds().clone()),
            ),
            account: Account { sender_address, state: AccountState { nonce: *tx.nonce() } },
        }))
        .return_once(|_| Ok(()));
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let app_state = app_state(Arc::new(mock_mempool_client), state_reader_factory);

    let response = add_tx(State(app_state), tx.into()).await.into_response();

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

fn calculate_hash(rpc_tx: &RpcTransaction) -> TransactionHash {
    let optional_class_info = match &rpc_tx {
        RpcTransaction::Declare(_declare_tx) => {
            panic!("Declare transactions are not supported in this test")
        }
        _ => None,
    };

    let account_tx = rpc_tx_to_account_tx(
        rpc_tx,
        optional_class_info,
        &ChainInfo::create_for_testing().chain_id,
    )
    .unwrap();
    account_tx.tx_hash()
}

// TODO(Arni): Remove this function. Replace with a function that take ownership of RpcTransaction.
fn rpc_tx_to_account_tx(
    rpc_tx: &RpcTransaction,
    // FIXME(yael 15/4/24): calculate class_info inside the function once compilation code is ready
    optional_class_info: Option<ClassInfo>,
    chain_id: &ChainId,
) -> StatefulTransactionValidatorResult<AccountTransaction> {
    match rpc_tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
            let declare_tx = DeclareTransaction::V3(DeclareTransactionV3 {
                class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                                   * function once ready */
                resource_bounds: ValidResourceBounds::AllResources(
                    rpc_tx.resource_bounds().clone(),
                ),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                compiled_class_hash: tx.compiled_class_hash,
                sender_address: tx.sender_address,
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
                account_deployment_data: tx.account_deployment_data.clone(),
            });
            let tx_hash = declare_tx
                .calculate_transaction_hash(chain_id, &declare_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            let class_info =
                optional_class_info.expect("declare transaction should contain class info");
            let declare_tx = BlockifierDeclareTransaction::new(declare_tx, tx_hash, class_info)
                .map_err(|e| {
                    error!("Failed to convert declare tx hash to blockifier tx type: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            Ok(AccountTransaction::Declare(declare_tx))
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
            let deploy_account_tx = DeployAccountTransaction::V3(DeployAccountTransactionV3 {
                resource_bounds: ValidResourceBounds::AllResources(
                    rpc_tx.resource_bounds().clone(),
                ),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                class_hash: tx.class_hash,
                contract_address_salt: tx.contract_address_salt,
                constructor_calldata: tx.constructor_calldata.clone(),
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
            });
            let contract_address = calculate_contract_address(
                deploy_account_tx.contract_address_salt(),
                deploy_account_tx.class_hash(),
                &deploy_account_tx.constructor_calldata(),
                ContractAddress::default(),
            )
            .map_err(|e| {
                error!("Failed to calculate contract address: {}", e);
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?;
            let tx_hash = deploy_account_tx
                .calculate_transaction_hash(chain_id, &deploy_account_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            let deploy_account_tx = BlockifierDeployAccountTransaction::new(
                deploy_account_tx,
                tx_hash,
                contract_address,
            );
            Ok(AccountTransaction::DeployAccount(deploy_account_tx))
        }
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
            let invoke_tx = starknet_api::transaction::InvokeTransaction::V3(InvokeTransactionV3 {
                resource_bounds: ValidResourceBounds::AllResources(
                    rpc_tx.resource_bounds().clone(),
                ),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                sender_address: tx.sender_address,
                calldata: tx.calldata.clone(),
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
                account_deployment_data: tx.account_deployment_data.clone(),
            });
            let tx_hash = invoke_tx
                .calculate_transaction_hash(chain_id, &invoke_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            let invoke_tx = BlockifierInvokeTransaction::new(invoke_tx, tx_hash);
            Ok(AccountTransaction::Invoke(invoke_tx))
        }
    }
}
