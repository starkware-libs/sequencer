use std::sync::Arc;

use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use apollo_class_manager_types::{
    EmptyClassManagerClient,
    MockClassManagerClient,
    SharedClassManagerClient,
};
use apollo_gateway_types::errors::GatewaySpecError;
use apollo_gateway_types::gateway_types::{
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
use mempool_test_utils::starknet_api_test_utils::{
    declare_tx,
    generate_deploy_account_with_salt,
    invoke_tx,
    VALID_ACCOUNT_BALANCE,
};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    RpcDeclareTransaction,
    RpcTransaction,
    RpcTransactionLabelValue,
};
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;
use strum::VariantNames;

use crate::config::{
    GatewayConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use crate::errors::GatewayResult;
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
    let class_manager_client = Arc::new(EmptyClassManagerClient);
    MockDependencies { config, state_reader_factory, mock_mempool_client, class_manager_client }
}

struct MockDependencies {
    config: GatewayConfig,
    state_reader_factory: TestStateReaderFactory,
    mock_mempool_client: MockMempoolClient,
    class_manager_client: SharedClassManagerClient,
}

impl MockDependencies {
    fn gateway(self) -> Gateway {
        register_metrics();
        let chain_id = self.config.chain_info.chain_id.clone();
        Gateway::new(
            self.config,
            Arc::new(self.state_reader_factory),
            Arc::new(self.mock_mempool_client),
            TransactionConverter::new(self.class_manager_client, chain_id),
        )
    }

    fn expect_add_tx(&mut self, args: AddTransactionArgsWrapper, result: MempoolClientResult<()>) {
        self.mock_mempool_client.expect_add_tx().once().with(eq(args)).return_once(|_| result);
    }
}

fn invoke() -> RpcTransaction {
    invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

async fn invoke_success() -> GatewayResult<GatewayOutput> {
    let tx = invoke();
    let internal = convert_rpc_tx_to_internal(
        &mock_dependencies(config(), state_reader_factory()),
        tx.clone(),
    )
    .await;
    Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(internal.tx_hash)))
}

/// Make a deploy account transaction with a default salt.
fn deploy_account() -> RpcTransaction {
    generate_deploy_account_with_salt(
        &FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        ContractAddressSalt(Felt::ZERO),
    )
}

async fn deploy_account_success() -> GatewayResult<GatewayOutput> {
    let tx = deploy_account();
    let address = tx.calculate_sender_address().unwrap();
    let internal = convert_rpc_tx_to_internal(
        &mock_dependencies(config(), state_reader_factory()),
        tx.clone(),
    )
    .await;
    Ok(GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(internal.tx_hash, address)))
}

async fn convert_rpc_tx_to_internal(
    mock_dependencies_object: &MockDependencies,
    rpc_tx: RpcTransaction,
) -> InternalRpcTransaction {
    let chain_id = mock_dependencies_object.config.chain_info.chain_id.clone();
    let class_manager_client = MockClassManagerClient::new();
    let tx_converter = TransactionConverter::new(Arc::new(class_manager_client), chain_id);
    tx_converter.convert_rpc_tx_to_internal_rpc_tx(rpc_tx).await.unwrap()
}

// TODO(AlonH): add test with Some broadcasted message metadata
// We use default nonce, address, and tx_hash since Gateway errors drop these details when
// converting Mempool errors.
#[rstest]
#[case::successful_invoke_transaction_addition(
    invoke(), Ok(()), invoke_success().await)]
#[case::invoke_tx_with_duplicate_tx_hash(
    invoke(),
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateTransaction { tx_hash: TransactionHash::default() })),
    Err(GatewaySpecError::DuplicateTx)
)]
#[case::invoke_tx_with_duplicate_nonce(
    invoke(),
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateNonce { address: ContractAddress::default(), nonce: Nonce::default() })),
    Err(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::invoke_tx_with_nonce_too_old(
    invoke(),
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooOld { address: ContractAddress::default(), nonce: Nonce::default() })),
    Err(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::invoke_tx_with_nonce_too_large(
    invoke(),
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooLarge(Nonce::default()))),
    Err(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::successful_deploy_account(
    deploy_account(),
    Ok(()),
    deploy_account_success().await
)]
#[tokio::test]
async fn test_add_tx(
    mut mock_dependencies: MockDependencies,
    #[case] tx: RpcTransaction,
    #[case] expected_mempool_result: Result<(), MempoolClientError>,
    #[case] expected_result: GatewayResult<GatewayOutput>,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let address = tx.calculate_sender_address().unwrap();

    fund_account(
        &mock_dependencies.config.chain_info,
        address,
        VALID_ACCOUNT_BALANCE,
        &mut mock_dependencies.state_reader_factory.state_reader.blockifier_state_reader,
    );

    let internal_tx: InternalRpcTransaction =
        convert_rpc_tx_to_internal(&mock_dependencies, tx.clone()).await;

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
    match expected_result {
        Err(expected_err) => {
            assert_eq!(
                metric_counters_for_queries.get_metric_value(TRANSACTIONS_FAILED, &metrics),
                1
            );
            assert_eq!(result.unwrap_err(), expected_err);
        }
        Ok(expected_output) => {
            assert_eq!(
                metric_counters_for_queries
                    .get_metric_value(TRANSACTIONS_SENT_TO_MEMPOOL, &metrics),
                1
            );

            assert_eq!(result.unwrap(), expected_output);
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
