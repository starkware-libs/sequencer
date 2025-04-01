use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use apollo_class_manager_types::{ClassHashes, EmptyClassManagerClient, MockClassManagerClient};
use apollo_gateway_types::errors::GatewaySpecError;
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
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_test_utils::{get_rng, GetTestInstance};
use assert_matches::assert_matches;
use blockifier::context::ChainInfo;
use blockifier::test_utils::initial_test_state::fund_account;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use mempool_test_utils::starknet_api_test_utils::{
    contract_class,
    declare_tx,
    generate_deploy_account_with_salt,
    invoke_tx,
    VALID_ACCOUNT_BALANCE,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    RpcDeclareTransaction,
    RpcTransaction,
    RpcTransactionLabelValue,
};
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{DeclareTransactionV3, TransactionHash};
use starknet_types_core::felt::Felt;
use strum::VariantNames;

use crate::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use crate::gateway::Gateway;
use crate::metrics::{
    register_metrics,
    GatewayMetricHandle,
    SourceLabelValue,
    GATEWAY_ADD_TX_LATENCY,
    LABEL_NAME_SOURCE,
    LABEL_NAME_TX_TYPE,
    TRANSACTIONS_FAILED,
    TRANSACTIONS_RECEIVED,
    TRANSACTIONS_SENT_TO_MEMPOOL,
};
use crate::state_reader_test_utils::{local_test_state_reader_factory, TestStateReaderFactory};

#[fixture]
fn config() -> GatewayConfig {
    GatewayConfig {
        stateless_tx_validator_config: StatelessTransactionValidatorConfig::default(),
        stateful_tx_validator_config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
        block_declare: false,
    }
}

#[fixture]
fn state_reader_factory() -> TestStateReaderFactory {
    local_test_state_reader_factory(CairoVersion::Cairo1(RunnableCairo1::Casm), true)
}

#[fixture]
fn mock_dependencies(
    config: GatewayConfig,
    state_reader_factory: TestStateReaderFactory,
) -> MockDependencies {
    let mock_mempool_client = MockMempoolClient::new();
    // TODO(noamsp): use MockTransactionConverter
    let mock_class_manager_client = MockClassManagerClient::new();
    MockDependencies {
        config,
        state_reader_factory,
        mock_mempool_client,
        mock_class_manager_client,
    }
}

struct MockDependencies {
    config: GatewayConfig,
    state_reader_factory: TestStateReaderFactory,
    mock_mempool_client: MockMempoolClient,
    mock_class_manager_client: MockClassManagerClient,
}

impl MockDependencies {
    fn gateway(self) -> Gateway {
        register_metrics();
        let chain_id = self.config.chain_info.chain_id.clone();
        Gateway::new(
            self.config,
            Arc::new(self.state_reader_factory),
            Arc::new(self.mock_mempool_client),
            TransactionConverter::new(Arc::new(self.mock_class_manager_client), chain_id),
        )
    }

    fn expect_add_tx(&mut self, args: AddTransactionArgsWrapper, result: MempoolClientResult<()>) {
        self.mock_mempool_client.expect_add_tx().once().with(eq(args)).return_once(|_| result);
    }
}

fn invoke() -> RpcTransaction {
    invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

/// Make a deploy account transaction with a default salt.
fn deploy_account() -> RpcTransaction {
    generate_deploy_account_with_salt(
        &FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        ContractAddressSalt(Felt::ZERO),
    )
}

fn declare() -> RpcTransaction {
    let mut tx = declare_tx();
    let declare_tx_v3 =
        assert_matches!(&mut tx, RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);
    declare_tx_v3.compiled_class_hash = get_contract_class_of_declare().compiled_class_hash();
    tx
}

fn get_contract_class_of_declare() -> ContractClass {
    let casm = CasmContractClass {
        prime: Default::default(),
        compiler_version: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    };
    let sierra_version = SierraVersion::default();
    ContractClass::V1((casm, sierra_version))
}

/// Setup MockClassManagerClient to expect the addition and retrieval of the test contract
/// class. Returns the compiled class hash of the contract class that the mock will return.
fn setup_class_manager_client_mock(mock_class_manager_client: &mut MockClassManagerClient) {
    let contract_class = contract_class();
    let class_hash = contract_class.calculate_class_hash();
    let class_hash_for_closure = class_hash;
    let executable = get_contract_class_of_declare();

    mock_class_manager_client
        .expect_add_class()
        .times(0..=1)
        .with(eq(contract_class.clone()))
        .return_once(move |_| {
            Ok(ClassHashes {
                class_hash: class_hash_for_closure,
                executable_class_hash: Default::default(),
            })
        });
    mock_class_manager_client
        .expect_get_sierra()
        .times(0..=1)
        .with(eq(class_hash))
        .return_once(move |_| Ok(Some(contract_class)));
    mock_class_manager_client
        .expect_get_executable()
        .times(0..=1)
        .with(eq(class_hash))
        .return_once(move |_| Ok(Some(executable)));
}

fn check_positive_add_tx_result(
    rpc_tx: RpcTransaction,
    tx_hash: TransactionHash,
    address: ContractAddress,
    result: GatewayOutput,
) {
    assert_eq!(
        result,
        match rpc_tx {
            RpcTransaction::Declare(tx) => {
                let tx = assert_matches!(tx, RpcDeclareTransaction::V3(tx) => tx);
                let tx = DeclareTransactionV3::from(tx);
                GatewayOutput::Declare(DeclareGatewayOutput::new(tx_hash, tx.class_hash))
            }
            RpcTransaction::DeployAccount(_) =>
                GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(tx_hash, address)),
            RpcTransaction::Invoke(_) => GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash)),
        }
    );
}

async fn convert_rpc_tx_to_internal(
    mock_dependencies_object: &MockDependencies,
    rpc_tx: RpcTransaction,
) -> InternalRpcTransaction {
    let chain_id = mock_dependencies_object.config.chain_info.chain_id.clone();
    let mut class_manager_client = MockClassManagerClient::new();
    if matches!(&rpc_tx, RpcTransaction::Declare(_)) {
        setup_class_manager_client_mock(&mut class_manager_client);
    }
    let tx_converter = TransactionConverter::new(Arc::new(class_manager_client), chain_id);
    tx_converter.convert_rpc_tx_to_internal_rpc_tx(rpc_tx).await.unwrap()
}

// TODO(AlonH): add test with Some broadcasted message metadata
// We use default nonce, address, and tx_hash since Gateway errors drop these details when
// converting Mempool errors.
// TODO(AndrewL): split into negative and positive tests
#[rstest]
#[case::successful_transaction(Ok(()), None)]
#[case::tx_with_duplicate_tx_hash(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateTransaction { tx_hash: TransactionHash::default() })),
    Some(GatewaySpecError::DuplicateTx)
)]
#[case::tx_with_duplicate_nonce(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateNonce { address: ContractAddress::default(), nonce: Nonce::default() })),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::tx_with_nonce_too_old(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooOld { address: ContractAddress::default(), nonce: Nonce::default() })),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::tx_with_nonce_too_large(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooLarge(Nonce::default()))),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
#[tokio::test]
async fn test_add_tx(
    mut mock_dependencies: MockDependencies,
    #[values(invoke(), deploy_account(), declare())] tx: RpcTransaction,
    #[case] expected_mempool_result: Result<(), MempoolClientError>,
    #[case] expected_error: Option<GatewaySpecError>,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let address = tx.calculate_sender_address().unwrap();

    setup_class_manager_client_mock(&mut mock_dependencies.mock_class_manager_client);

    fund_account(
        &mock_dependencies.config.chain_info,
        address,
        VALID_ACCOUNT_BALANCE,
        &mut mock_dependencies.state_reader_factory.state_reader.blockifier_state_reader,
    );

    let internal_tx: InternalRpcTransaction =
        convert_rpc_tx_to_internal(&mock_dependencies, tx.clone()).await;
    let tx_hash = internal_tx.tx_hash();

    let p2p_message_metadata = Some(BroadcastedMessageMetadata::get_test_instance(&mut get_rng()));
    let add_tx_args = AddTransactionArgs {
        tx: internal_tx,
        account_state: AccountState { address, nonce: *tx.nonce() },
    };
    mock_dependencies.expect_add_tx(
        AddTransactionArgsWrapper {
            args: add_tx_args,
            p2p_message_metadata: p2p_message_metadata.clone(),
        },
        expected_mempool_result,
    );

    let gateway = mock_dependencies.gateway();

    let result = gateway.add_tx(tx.clone(), p2p_message_metadata.clone()).await;

    let metric_counters_for_queries = GatewayMetricHandle::new(&tx, &p2p_message_metadata);
    let metrics = recorder.handle().render();
    assert_eq!(metric_counters_for_queries.get_metric_value(TRANSACTIONS_RECEIVED, &metrics), 1);
    match expected_error {
        Some(expected_err) => {
            assert_eq!(
                metric_counters_for_queries.get_metric_value(TRANSACTIONS_FAILED, &metrics),
                1
            );
            assert_eq!(result.unwrap_err(), expected_err);
        }
        None => {
            assert_eq!(
                metric_counters_for_queries
                    .get_metric_value(TRANSACTIONS_SENT_TO_MEMPOOL, &metrics),
                1
            );
            check_positive_add_tx_result(tx, tx_hash, address, result.unwrap());
        }
    }
}

// Gateway spec errors tests.
// TODO(Arni): Add tests for all the error cases. Check the response (use `into_response` on the
// result of `add_tx`).
// TODO(shahak): Test that when an error occurs in handle_request, then it returns the given p2p
// metadata.
// TODO(noamsp): Remove ignore from compiled_class_hash_mismatch once class manager component is
// implemented.
#[rstest]
#[tokio::test]
#[ignore]
async fn test_compiled_class_hash_mismatch(mock_dependencies: MockDependencies) {
    let mut declare_tx =
        assert_matches!(declare_tx(), RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);
    declare_tx.compiled_class_hash = CompiledClassHash::default();
    let tx = RpcTransaction::Declare(RpcDeclareTransaction::V3(declare_tx));

    let gateway = mock_dependencies.gateway();

    let err = gateway.add_tx(tx, None).await.unwrap_err();
    assert_matches!(err, GatewaySpecError::CompiledClassHashMismatch);
}

#[rstest]
#[tokio::test]
async fn test_block_declare_config(
    mut config: GatewayConfig,
    state_reader_factory: TestStateReaderFactory,
) {
    config.block_declare = true;
    let gateway = Gateway::new(
        config,
        Arc::new(state_reader_factory),
        Arc::new(MockMempoolClient::new()),
        TransactionConverter::new(
            Arc::new(EmptyClassManagerClient),
            ChainInfo::create_for_testing().chain_id,
        ),
    );

    let result = gateway.add_tx(declare_tx(), None).await;
    assert_eq!(
        result.unwrap_err(),
        GatewaySpecError::UnexpectedError {
            data: "Transaction type is temporarily blocked.".to_string()
        }
    );
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

            assert_eq!(
                TRANSACTIONS_RECEIVED.parse_numeric_metric::<u64>(&metrics, labels).unwrap(),
                0
            );
            assert_eq!(
                TRANSACTIONS_FAILED.parse_numeric_metric::<u64>(&metrics, labels).unwrap(),
                0
            );
            assert_eq!(
                TRANSACTIONS_SENT_TO_MEMPOOL.parse_numeric_metric::<u64>(&metrics, labels).unwrap(),
                0
            );
            assert_eq!(GATEWAY_ADD_TX_LATENCY.parse_histogram_metric(&metrics).unwrap().sum, 0.0);
            assert_eq!(GATEWAY_ADD_TX_LATENCY.parse_histogram_metric(&metrics).unwrap().count, 0);
        }
    }
}
