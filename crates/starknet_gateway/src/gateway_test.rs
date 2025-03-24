use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{declare_tx, invoke_tx};
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate::eq;
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rstest::{fixture, rstest};
use starknet_api::core::{CompiledClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcTransaction,
    RpcTransactionLabelValue,
};
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use starknet_api::transaction::{
    InvokeTransaction,
    TransactionHash,
    TransactionHasher,
    TransactionVersion,
};
use starknet_class_manager_types::transaction_converter::TransactionConverter;
use starknet_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::{
    AddTransactionArgsWrapper,
    MempoolClientError,
    MempoolClientResult,
    MockMempoolClient,
};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
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
    }
}

#[fixture]
fn state_reader_factory() -> TestStateReaderFactory {
    local_test_state_reader_factory(CairoVersion::Cairo1(RunnableCairo1::Casm), false)
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

type SenderAddress = ContractAddress;

fn create_tx() -> (RpcTransaction, SenderAddress) {
    let tx = invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let sender_address = match &tx {
        RpcTransaction::Invoke(starknet_api::rpc_transaction::RpcInvokeTransaction::V3(
            invoke_tx,
        )) => invoke_tx.sender_address,
        _ => panic!("Unexpected transaction type"),
    };
    (tx, sender_address)
}

// TODO(AlonH): add test with Some broadcasted message metadata
// We use default nonce, address, and tx_hash since Gateway errors drop these details when
// converting Mempool errors.
#[rstest]
#[case::successful_transaction_addition(Ok(()), None)]
#[case::duplicate_tx_hash(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateTransaction { tx_hash: TransactionHash::default() })),
    Some(GatewaySpecError::DuplicateTx)
)]
#[case::duplicate_nonce(
    Err(MempoolClientError::MempoolError(MempoolError::DuplicateNonce { address: ContractAddress::default(), nonce: Nonce::default() })),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::nonce_too_old(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooOld { address: ContractAddress::default(), nonce: Nonce::default() })),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
#[case::nonce_too_large(
    Err(MempoolClientError::MempoolError(MempoolError::NonceTooLarge(Nonce::default()))),
    Some(GatewaySpecError::InvalidTransactionNonce)
)]
// TODO(alonl): test add_txs with multiple txs
#[tokio::test]
async fn test_add_tx(
    mut mock_dependencies: MockDependencies,
    #[case] expected_result: Result<(), MempoolClientError>,
    #[case] expected_error: Option<GatewaySpecError>,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let (rpc_tx, address) = create_tx();
    let rpc_invoke_tx =
        assert_matches!(rpc_tx.clone(), RpcTransaction::Invoke(rpc_invoke_tx) => rpc_invoke_tx);

    let InvokeTransaction::V3(invoke_tx): InvokeTransaction = rpc_invoke_tx.clone().into() else {
        panic!("Unexpected transaction version")
    };

    let tx_hash = invoke_tx
        .calculate_transaction_hash(&CHAIN_ID_FOR_TESTS, &TransactionVersion::THREE)
        .unwrap();

    let internal_invoke_tx = InternalRpcTransaction {
        tx: InternalRpcTransactionWithoutTxHash::Invoke(rpc_invoke_tx),
        tx_hash,
    };

    let p2p_message_metadata = Some(BroadcastedMessageMetadata::get_test_instance(&mut get_rng()));
    let add_tx_args = AddTransactionArgs {
        tx: internal_invoke_tx,
        account_state: AccountState { address, nonce: *rpc_tx.nonce() },
    };
    mock_dependencies.expect_add_tx(
        AddTransactionArgsWrapper {
            args: add_tx_args,
            p2p_message_metadata: p2p_message_metadata.clone(),
        },
        expected_result,
    );

    let gateway = mock_dependencies.gateway();

    let result = gateway.add_txs(vec![rpc_tx.clone()], p2p_message_metadata.clone()).await;

    let metric_counters_for_queries = GatewayMetricHandle::new(&rpc_tx, &p2p_message_metadata);
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
            assert_eq!(result.unwrap(), vec![tx_hash]);
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

    let err = gateway.add_txs(vec![tx], None).await.unwrap_err();
    assert_matches!(err, GatewaySpecError::CompiledClassHashMismatch);
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
        }
    }
}
