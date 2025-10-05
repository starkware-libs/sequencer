use std::collections::HashSet;
use std::fs::File;
use std::sync::{Arc, LazyLock};

use apollo_class_manager_types::transaction_converter::{
    MockTransactionConverterTrait,
    TransactionConverterError,
    TransactionConverterResult,
};
use apollo_config::dumping::SerializeConfig;
use apollo_config::loading::load_and_process_config;
use apollo_gateway_config::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::gateway_types::{
    DeclareGatewayOutput,
    DeployAccountGatewayOutput,
    GatewayOutput,
    InvokeGatewayOutput,
};
use apollo_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolClientError,
    MempoolClientResult,
    MockMempoolClient,
};
use apollo_mempool_types::errors::MempoolError;
use apollo_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use apollo_metrics::metrics::HistogramValue;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_test_utils::{get_rng, GetTestInstance};
use blockifier::context::ChainInfo;
use blockifier::test_utils::initial_test_state::fund_account;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_trivial_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use clap::Command;
use mempool_test_utils::starknet_api_test_utils::{
    contract_class,
    declare_tx,
    test_valid_resource_bounds,
    VALID_ACCOUNT_BALANCE,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    RpcDeclareTransaction,
    RpcTransaction,
    RpcTransactionLabelValue,
};
use starknet_api::test_utils::declare::{
    default_compiled_contract_class,
    DeclareTxArgsWithContractClass,
};
use starknet_api::test_utils::deploy_account::DeployAccountTxArgs;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{TestingTxArgs, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::TransactionSignature;
use starknet_api::transaction::TransactionHash;
use starknet_api::{
    contract_address,
    declare_tx_args,
    deploy_account_tx_args,
    invoke_tx_args,
    nonce,
};
use starknet_types_core::felt::Felt;
use strum::VariantNames;
use tempfile::TempDir;

use crate::errors::{GatewayResult, StatelessTransactionValidatorError};
use crate::gateway::{Gateway, ProcessTxBlockingTask};
use crate::metrics::{
    register_metrics,
    GatewayMetricHandle,
    SourceLabelValue,
    GATEWAY_ADD_TX_LATENCY,
    GATEWAY_TRANSACTIONS_FAILED,
    GATEWAY_TRANSACTIONS_RECEIVED,
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE,
};
use crate::state_reader::MockStateReaderFactory;
use crate::state_reader_test_utils::{local_test_state_reader_factory, TestStateReaderFactory};
use crate::stateful_transaction_validator::{
    MockStatefulTransactionValidatorFactoryTrait,
    MockStatefulTransactionValidatorTrait,
};
use crate::stateless_transaction_validator::MockStatelessTransactionValidatorTrait;

#[fixture]
fn mock_stateful_transaction_validator() -> MockStatefulTransactionValidatorTrait {
    MockStatefulTransactionValidatorTrait::new()
}

#[fixture]
fn mock_stateful_transaction_validator_factory() -> MockStatefulTransactionValidatorFactoryTrait {
    MockStatefulTransactionValidatorFactoryTrait::new()
}

#[fixture]
fn mock_stateless_transaction_validator() -> MockStatelessTransactionValidatorTrait {
    let mut mock_stateless_transaction_validator = MockStatelessTransactionValidatorTrait::new();
    mock_stateless_transaction_validator.expect_validate().return_once(|_| Ok(()));
    mock_stateless_transaction_validator
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    let config = GatewayConfig {
        stateless_tx_validator_config: StatelessTransactionValidatorConfig::default(),
        stateful_tx_validator_config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
        block_declare: false,
        authorized_declarer_accounts: None,
    };
    let state_reader_factory =
        local_test_state_reader_factory(CairoVersion::Cairo1(RunnableCairo1::Casm), true);
    let mock_mempool_client = MockMempoolClient::new();
    let mock_transaction_converter = MockTransactionConverterTrait::new();
    let mock_stateless_transaction_validator = mock_stateless_transaction_validator();
    MockDependencies {
        config,
        state_reader_factory,
        mock_mempool_client,
        mock_transaction_converter,
        mock_stateless_transaction_validator,
    }
}

struct MockDependencies {
    config: GatewayConfig,
    state_reader_factory: TestStateReaderFactory,
    mock_mempool_client: MockMempoolClient,
    mock_transaction_converter: MockTransactionConverterTrait,
    mock_stateless_transaction_validator: MockStatelessTransactionValidatorTrait,
}

impl MockDependencies {
    fn gateway(self) -> Gateway {
        register_metrics();
        Gateway::new(
            self.config,
            Arc::new(self.state_reader_factory),
            Arc::new(self.mock_mempool_client),
            Arc::new(self.mock_transaction_converter),
            Arc::new(self.mock_stateless_transaction_validator),
        )
    }

    fn expect_add_tx(&mut self, args: AddTransactionArgsWrapper, result: MempoolClientResult<()>) {
        self.mock_mempool_client.expect_add_tx().once().with(eq(args)).return_once(|_| result);
    }
}

fn account_contract() -> FeatureContract {
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

fn invoke_args() -> InvokeTxArgs {
    let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let mut args = invoke_tx_args!(
        resource_bounds: test_valid_resource_bounds(),
        sender_address: account_contract().get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0))
    );
    let internal_tx = args.get_internal_tx();
    args.tx_hash = internal_tx.tx.calculate_transaction_hash(&CHAIN_ID_FOR_TESTS).unwrap();
    args
}

/// Make a deploy account transaction with a default salt.
fn deploy_account_args() -> DeployAccountTxArgs {
    let mut args = deploy_account_tx_args!(
        class_hash: account_contract().get_class_hash(),
        resource_bounds: test_valid_resource_bounds(),
    );
    let internal_tx = args.get_internal_tx();
    args.tx_hash = internal_tx.tx.calculate_transaction_hash(&CHAIN_ID_FOR_TESTS).unwrap();
    args
}

fn declare_args() -> DeclareTxArgsWithContractClass {
    let contract_class = contract_class();
    let mut args = DeclareTxArgsWithContractClass {
        args: declare_tx_args!(
            signature: TransactionSignature(vec![Felt::ZERO].into()),
            sender_address: account_contract().get_instance_address(0),
            resource_bounds: test_valid_resource_bounds(),
            class_hash: contract_class.calculate_class_hash(),
            compiled_class_hash: default_compiled_contract_class().compiled_class_hash(),
        ),
        contract_class,
    };
    let internal_tx = args.get_internal_tx();
    args.args.tx_hash = internal_tx.tx.calculate_transaction_hash(&CHAIN_ID_FOR_TESTS).unwrap();
    args
}

fn setup_transaction_converter_mock(
    mock_transaction_converter: &mut MockTransactionConverterTrait,
    tx_args: &impl TestingTxArgs,
) {
    let rpc_tx = tx_args.get_rpc_tx();
    let internal_tx = tx_args.get_internal_tx();
    mock_transaction_converter
        .expect_convert_rpc_tx_to_internal_rpc_tx()
        .once()
        .with(eq(rpc_tx))
        .return_once(move |_| Ok(internal_tx));

    let internal_tx = tx_args.get_internal_tx();
    let executable_tx = tx_args.get_executable_tx();
    mock_transaction_converter
        .expect_convert_internal_rpc_tx_to_executable_tx()
        .once()
        .with(eq(internal_tx))
        .return_once(move |_| Ok(executable_tx));
}

fn check_positive_add_tx_result(tx_args: impl TestingTxArgs, result: GatewayOutput) {
    let rpc_tx = tx_args.get_rpc_tx();
    let expected_internal_tx = tx_args.get_internal_tx();
    let tx_hash = expected_internal_tx.tx_hash();
    assert_eq!(
        result,
        match rpc_tx {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
                GatewayOutput::Declare(DeclareGatewayOutput::new(
                    tx_hash,
                    tx.contract_class.calculate_class_hash(),
                ))
            }
            RpcTransaction::DeployAccount(_) => {
                let address = expected_internal_tx.contract_address();
                GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(tx_hash, address))
            }
            RpcTransaction::Invoke(_) => GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash)),
        }
    );
}

static P2P_MESSAGE_METADATA: LazyLock<Option<BroadcastedMessageMetadata>> =
    LazyLock::new(|| Some(BroadcastedMessageMetadata::get_test_instance(&mut get_rng())));
fn p2p_message_metadata() -> Option<BroadcastedMessageMetadata> {
    P2P_MESSAGE_METADATA.clone()
}

async fn setup_mock_state(
    mock_dependencies: &mut MockDependencies,
    tx_args: &impl TestingTxArgs,
    expected_mempool_result: Result<(), MempoolClientError>,
) {
    let input_tx = tx_args.get_rpc_tx();
    let expected_internal_tx = tx_args.get_internal_tx();

    setup_transaction_converter_mock(&mut mock_dependencies.mock_transaction_converter, tx_args);

    let address = expected_internal_tx.contract_address();
    fund_account(
        &mock_dependencies.config.chain_info,
        address,
        VALID_ACCOUNT_BALANCE,
        &mut mock_dependencies.state_reader_factory.state_reader.blockifier_state_reader,
    );

    let mempool_add_tx_args = AddTransactionArgs {
        tx: expected_internal_tx.clone(),
        account_state: AccountState { address, nonce: *input_tx.nonce() },
    };
    mock_dependencies.expect_add_tx(
        AddTransactionArgsWrapper {
            args: mempool_add_tx_args,
            p2p_message_metadata: p2p_message_metadata(),
        },
        expected_mempool_result,
    );
}

struct AddTxResults {
    result: GatewayResult<GatewayOutput>,
    metric_handle_for_queries: GatewayMetricHandle,
    metrics: String,
}

async fn run_add_tx_and_extract_metrics(
    mock_dependencies: MockDependencies,
    tx_args: &impl TestingTxArgs,
) -> AddTxResults {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let input_tx = tx_args.get_rpc_tx();
    let gateway = mock_dependencies.gateway();
    let result = gateway.add_tx(input_tx.clone(), p2p_message_metadata()).await;

    let metric_handle_for_queries = GatewayMetricHandle::new(&input_tx, &p2p_message_metadata());
    let metrics = recorder.handle().render();

    AddTxResults { result, metric_handle_for_queries, metrics }
}

fn process_tx_task(
    stateful_transaction_validator_factory: MockStatefulTransactionValidatorFactoryTrait,
) -> ProcessTxBlockingTask {
    ProcessTxBlockingTask {
        stateful_tx_validator_factory: Arc::new(stateful_transaction_validator_factory),
        state_reader_factory: Arc::new(MockStateReaderFactory::new()),
        mempool_client: Arc::new(MockMempoolClient::new()),
        executable_tx: executable_invoke_tx(invoke_args()),
        runtime: tokio::runtime::Handle::current(),
    }
}

// Gateway spec errors tests.
// TODO(Arni): Add tests for all the error cases. Check the response (use `into_response` on the
// result of `add_tx`).
// TODO(shahak): Test that when an error occurs in handle_request, then it returns the given p2p
// metadata.
// TODO(AlonH): add test with Some broadcasted message metadata
#[rstest]
#[case::tx_with_duplicate_tx_hash(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateTransaction { tx_hash: TransactionHash::default() })),
    StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::DuplicatedTransaction)
)]
#[case::tx_with_duplicate_nonce(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateNonce { address: ContractAddress::default(), nonce: Nonce::default() })),
    StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce)
)]
#[case::tx_with_nonce_too_old(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooOld { address: ContractAddress::default(), tx_nonce: Nonce::default(), account_nonce: nonce!(1) })),
    StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce)
)]
#[case::tx_with_nonce_too_large(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooLarge(Nonce::default()))),
    StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.NONCE_TOO_LARGE".to_string())
)]
#[tokio::test]
async fn test_add_tx_negative(
    mut mock_dependencies: MockDependencies,
    #[values(invoke_args(), deploy_account_args(), declare_args())] tx_args: impl TestingTxArgs,
    #[case] expected_mempool_result: Result<(), MempoolClientError>,
    #[case] expected_error_code: StarknetErrorCode,
) {
    setup_mock_state(&mut mock_dependencies, &tx_args, expected_mempool_result).await;

    let AddTxResults { result, metric_handle_for_queries, metrics } =
        run_add_tx_and_extract_metrics(mock_dependencies, &tx_args).await;

    assert_eq!(
        metric_handle_for_queries.get_metric_value(GATEWAY_TRANSACTIONS_RECEIVED, &metrics),
        1
    );
    assert_eq!(
        metric_handle_for_queries.get_metric_value(GATEWAY_TRANSACTIONS_FAILED, &metrics),
        1
    );
    assert_eq!(result.unwrap_err().code, expected_error_code);
}

#[rstest]
#[tokio::test]
async fn test_add_tx_positive(
    mut mock_dependencies: MockDependencies,
    #[values(invoke_args(), deploy_account_args(), declare_args())] tx_args: impl TestingTxArgs,
) {
    setup_mock_state(&mut mock_dependencies, &tx_args, Ok(())).await;

    let AddTxResults { result, metric_handle_for_queries, metrics } =
        run_add_tx_and_extract_metrics(mock_dependencies, &tx_args).await;

    assert_eq!(
        metric_handle_for_queries.get_metric_value(GATEWAY_TRANSACTIONS_RECEIVED, &metrics),
        1
    );
    assert_eq!(
        metric_handle_for_queries.get_metric_value(GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL, &metrics),
        1
    );
    check_positive_add_tx_result(tx_args, result.unwrap());
}

#[rstest]
#[case::rpc_to_internal_fails(
    Err(TransactionConverterError::ClassNotFound { class_hash: ClassHash::default() }),
    // This value is never used because the first step already fails. Provided a valid executable tx to satisfy the signature.
    Ok(executable_invoke_tx(invoke_args())),
)]
#[case::internal_to_executable_fails(
    Ok(invoke_args().get_internal_tx()),
    Err(TransactionConverterError::ClassNotFound { class_hash: ClassHash::default() })
)]
#[tokio::test]
async fn test_transaction_converter_error(
    #[case] expect_internal_rpc_tx_result: TransactionConverterResult<InternalRpcTransaction>,
    #[case] expect_executable_tx_result: TransactionConverterResult<AccountTransaction>,
    mut mock_dependencies: MockDependencies,
) {
    mock_dependencies.mock_mempool_client.expect_add_tx().never();
    mock_dependencies
        .mock_transaction_converter
        .expect_convert_rpc_tx_to_internal_rpc_tx()
        .return_once(|_| expect_internal_rpc_tx_result);
    mock_dependencies
        .mock_transaction_converter
        .expect_convert_internal_rpc_tx_to_executable_tx()
        .return_once(|_| expect_executable_tx_result);

    let gateway = mock_dependencies.gateway();

    let err = gateway.add_tx(declare_tx(), None).await.unwrap_err();

    // All TransactionConverter errors are mapped to InternalError.
    assert_eq!(
        err.code,
        StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.InternalError".into())
    );
}

#[rstest]
#[tokio::test]
async fn test_block_declare_config(mut mock_dependencies: MockDependencies) {
    mock_dependencies.config.block_declare = true;
    let gateway = mock_dependencies.gateway();

    let result = gateway.add_tx(declare_tx(), None).await;
    let expected_code = StarknetErrorCode::UnknownErrorCode(
        "StarknetErrorCode.BLOCKED_TRANSACTION_TYPE".to_string(),
    );
    assert_eq!(result.unwrap_err().code, expected_code);
}

#[test]
fn test_register_metrics() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();
    let metrics = recorder.handle().render();
    for tx_type in RpcTransactionLabelValue::VARIANTS {
        for source in SourceLabelValue::VARIANTS {
            let labels: &[(&str, &str); 2] =
                &[(LABEL_NAME_TX_TYPE, tx_type), (LABEL_NAME_SOURCE, source)];

            // TODO(Tsabary): replace with assert_exists when available.
            assert_eq!(
                GATEWAY_TRANSACTIONS_RECEIVED
                    .parse_numeric_metric::<u64>(&metrics, labels)
                    .unwrap(),
                0
            );
            assert_eq!(
                GATEWAY_TRANSACTIONS_FAILED.parse_numeric_metric::<u64>(&metrics, labels).unwrap(),
                0
            );
            assert_eq!(
                GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL
                    .parse_numeric_metric::<u64>(&metrics, labels)
                    .unwrap(),
                0
            );
            GATEWAY_ADD_TX_LATENCY.assert_eq(&metrics, &HistogramValue::default());
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_unauthorized_declare_config(mut mock_dependencies: MockDependencies) {
    let authorized_address = contract_address!("0x1");
    mock_dependencies.config.authorized_declarer_accounts = Some(vec![authorized_address]);

    let gateway = mock_dependencies.gateway();
    let rpc_declare_tx = declare_tx();

    // Ensure the sender address is different from the authorized address.
    assert_ne!(
        rpc_declare_tx.calculate_sender_address().unwrap(),
        authorized_address,
        "Sender address should not be authorized"
    );

    let gateway_output_code_error = gateway.add_tx(rpc_declare_tx, None).await.unwrap_err().code;
    let expected_code_error =
        StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::UnauthorizedDeclare);

    assert_eq!(gateway_output_code_error, expected_code_error);
}

#[rstest]
#[case::two_addresses(
    Some(vec![
        contract_address!("0x1"),
        contract_address!("0x2"),
    ])
)]
#[case::one_address(
    Some(vec![
        contract_address!("0x1"),
    ])
)]
#[case::none(None)]
fn test_full_cycle_dump_deserialize_authorized_declarer_accounts(
    #[case] authorized_declarer_accounts: Option<Vec<ContractAddress>>,
) {
    let original_config = GatewayConfig { authorized_declarer_accounts, ..Default::default() };

    // Create a temporary file to dump the config.
    let file_path = TempDir::new().unwrap().path().join("config.json");
    original_config.dump_to_file(&vec![], &HashSet::new(), file_path.to_str().unwrap()).unwrap();

    // Load the config from the dumped config file.
    let loaded_config = load_and_process_config::<GatewayConfig>(
        File::open(file_path).unwrap(), // Config file to load.
        Command::new(""),               // Unused CLI context.
        vec![],                         // No override CLI args.
        false,                          // Use schema defaults.
    )
    .unwrap();

    assert_eq!(loaded_config, original_config);
}

#[rstest]
#[case::validate_failure(StarknetErrorCode::KnownErrorCode(
    KnownStarknetErrorCode::ValidateFailure
))]
#[case::invalid_nonce(StarknetErrorCode::KnownErrorCode(
    KnownStarknetErrorCode::InvalidTransactionNonce
))]
#[case::gas_price_too_low(
    StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.GAS_PRICE_TOO_LOW".into())
)]
#[case::internal_error(
    StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.InternalError".into())
)]
#[tokio::test]
async fn process_tx_returns_error_when_extract_state_nonce_and_run_validations_fails(
    #[case] error_code: StarknetErrorCode,
    mut mock_stateful_transaction_validator: MockStatefulTransactionValidatorTrait,
    mut mock_stateful_transaction_validator_factory: MockStatefulTransactionValidatorFactoryTrait,
) {
    let expected_error = StarknetError {
        code: error_code.clone(),
        message: "placeholder".into(), // Message is not checked
    };

    mock_stateful_transaction_validator
        .expect_extract_state_nonce_and_run_validations()
        .return_once(|_, _, _| Err(expected_error));

    mock_stateful_transaction_validator_factory
        .expect_instantiate_validator()
        .return_once(|_| Ok(Box::new(mock_stateful_transaction_validator)));

    let process_tx_task = process_tx_task(mock_stateful_transaction_validator_factory);

    let result = tokio::task::spawn_blocking(move || process_tx_task.process_tx()).await.unwrap();

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, error_code);
}

#[rstest]
#[tokio::test]
async fn stateless_transaction_validator_error(mut mock_dependencies: MockDependencies) {
    let arbitrary_validation_error = Err(StatelessTransactionValidatorError::SignatureTooLong {
        signature_length: 5001,
        max_signature_length: 4000,
    });
    let error_code =
        StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.SIGNATURE_TOO_LONG".into());
    let mut mock_stateless_transaction_validator = MockStatelessTransactionValidatorTrait::new();
    mock_stateless_transaction_validator
        .expect_validate()
        .return_once(|_| arbitrary_validation_error);
    mock_dependencies.mock_stateless_transaction_validator = mock_stateless_transaction_validator;
    let gateway = mock_dependencies.gateway();
    let result = gateway.add_tx(invoke_args().get_rpc_tx(), None).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, error_code);
}

#[rstest]
#[tokio::test]
async fn process_tx_returns_error_when_instantiating_validator_fails(
    mut mock_stateful_transaction_validator_factory: MockStatefulTransactionValidatorFactoryTrait,
) {
    let error_code = StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.InternalError".into());
    let expected_error = StarknetError {
        code: error_code.clone(),
        message: "placeholder".into(), // Message is not checked
    };
    mock_stateful_transaction_validator_factory
        .expect_instantiate_validator()
        .return_once(|_| Err(expected_error));

    let process_tx_task = process_tx_task(mock_stateful_transaction_validator_factory);

    let result = tokio::task::spawn_blocking(move || process_tx_task.process_tx()).await.unwrap();

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, error_code);
}
