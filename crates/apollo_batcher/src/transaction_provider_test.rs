use std::sync::Arc;

use apollo_l1_provider_types::{
    InvalidValidationStatus,
    MockL1ProviderClient,
    ValidationStatus as L1ValidationStatus,
};
use apollo_mempool_types::communication::MockMempoolClient;
use assert_matches::assert_matches;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::test_utils::invoke::{internal_invoke_tx, InvokeTxArgs};
use starknet_api::tx_hash;

use crate::transaction_provider::{
    ProposeTransactionProvider,
    TransactionProvider,
    TransactionProviderError,
    ValidateTransactionProvider,
};

const MAX_L1_HANDLER_TXS_PER_BLOCK: usize = 15;
const HEIGHT: BlockNumber = BlockNumber(1);
const MAX_TXS_PER_FETCH: usize = 10;
const VALIDATE_BUFFER_SIZE: usize = 30;

struct MockDependencies {
    mempool_client: MockMempoolClient,
    l1_provider_client: MockL1ProviderClient,
    tx_sender: tokio::sync::mpsc::Sender<InternalConsensusTransaction>,
    tx_receiver: tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
    final_n_executed_txs_sender: tokio::sync::oneshot::Sender<usize>,
    final_n_executed_txs_receiver: tokio::sync::oneshot::Receiver<usize>,
}

impl MockDependencies {
    fn expect_get_l1_handler_txs(&mut self, n_to_request: usize, n_to_return: usize) {
        self.l1_provider_client
            .expect_get_txs()
            .with(eq(n_to_request), eq(HEIGHT))
            .returning(move |_, _| Ok(vec![L1HandlerTransaction::default(); n_to_return]));
    }

    fn expect_get_mempool_txs(&mut self, n_to_request: usize) {
        self.mempool_client.expect_get_txs().with(eq(n_to_request)).returning(move |n_requested| {
            Ok(vec![internal_invoke_tx(InvokeTxArgs::default()); n_requested])
        });
    }

    fn expect_validate_l1handler(&mut self, tx: L1HandlerTransaction, result: L1ValidationStatus) {
        self.l1_provider_client
            .expect_validate()
            .withf(move |tx_arg, height| tx_arg == &tx.tx_hash && *height == HEIGHT)
            .returning(move |_, _| Ok(result));
    }

    async fn simulate_input_txs(&mut self, txs: Vec<InternalConsensusTransaction>) {
        for tx in txs {
            self.tx_sender.send(tx).await.unwrap();
        }
    }

    fn propose_tx_provider(self) -> ProposeTransactionProvider {
        ProposeTransactionProvider::new(
            Arc::new(self.mempool_client),
            Arc::new(self.l1_provider_client),
            MAX_L1_HANDLER_TXS_PER_BLOCK,
            HEIGHT,
        )
    }

    fn validate_tx_provider(self) -> ValidateTransactionProvider {
        self.validate_tx_provider_with_final_n_executed_txs().0
    }

    fn validate_tx_provider_with_final_n_executed_txs(
        self,
    ) -> (ValidateTransactionProvider, tokio::sync::oneshot::Sender<usize>) {
        let validate_tx_provider = ValidateTransactionProvider::new(
            self.tx_receiver,
            self.final_n_executed_txs_receiver,
            Arc::new(self.l1_provider_client),
            HEIGHT,
        );
        (validate_tx_provider, self.final_n_executed_txs_sender)
    }
}

#[fixture]
fn mock_dependencies(
    tx_channel: (
        tokio::sync::mpsc::Sender<InternalConsensusTransaction>,
        tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
    ),
    final_n_executed_txs_channel: (
        tokio::sync::oneshot::Sender<usize>,
        tokio::sync::oneshot::Receiver<usize>,
    ),
) -> MockDependencies {
    let (tx_sender, tx_receiver) = tx_channel;
    let (final_n_executed_txs_sender, final_n_executed_txs_receiver) = final_n_executed_txs_channel;
    MockDependencies {
        mempool_client: MockMempoolClient::new(),
        l1_provider_client: MockL1ProviderClient::new(),
        tx_sender,
        tx_receiver,
        final_n_executed_txs_sender,
        final_n_executed_txs_receiver,
    }
}

#[fixture]
fn tx_channel() -> (
    tokio::sync::mpsc::Sender<InternalConsensusTransaction>,
    tokio::sync::mpsc::Receiver<InternalConsensusTransaction>,
) {
    tokio::sync::mpsc::channel(VALIDATE_BUFFER_SIZE)
}

#[fixture]
fn final_n_executed_txs_channel()
-> (tokio::sync::oneshot::Sender<usize>, tokio::sync::oneshot::Receiver<usize>) {
    tokio::sync::oneshot::channel()
}

fn test_l1handler_tx() -> L1HandlerTransaction {
    L1HandlerTransaction { tx_hash: tx_hash!(1), ..Default::default() }
}

#[rstest]
#[tokio::test]
async fn fill_max_l1_handler(mut mock_dependencies: MockDependencies) {
    // Set values so fetches will be done in multiple steps:
    // 1. Fetch 10 l1 handler transactions.
    // 2. Fetch 5 l1 handler transactions (reach max_l1_handler_txs_per_block) + 5 mempool txs.
    // 3. Fetch 10 mempool txs.
    mock_dependencies.expect_get_l1_handler_txs(MAX_TXS_PER_FETCH, MAX_TXS_PER_FETCH);
    let n_l1handler_left = MAX_L1_HANDLER_TXS_PER_BLOCK - MAX_TXS_PER_FETCH;
    mock_dependencies.expect_get_l1_handler_txs(n_l1handler_left, n_l1handler_left);
    let n_mempool_left_after_l1 = MAX_TXS_PER_FETCH - n_l1handler_left;
    mock_dependencies.expect_get_mempool_txs(n_mempool_left_after_l1);
    mock_dependencies.expect_get_mempool_txs(MAX_TXS_PER_FETCH);

    let mut tx_provider = mock_dependencies.propose_tx_provider();

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, InternalConsensusTransaction::L1Handler(_))));

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(
        data[..n_l1handler_left]
            .iter()
            .all(|tx| matches!(tx, InternalConsensusTransaction::L1Handler(_)))
    );
    assert!(
        data[n_l1handler_left..]
            .iter()
            .all(|tx| matches!(tx, InternalConsensusTransaction::RpcTransaction(_)))
    );

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, InternalConsensusTransaction::RpcTransaction(_))));
}

#[rstest]
#[tokio::test]
async fn no_more_l1_handler(mut mock_dependencies: MockDependencies) {
    // Request more l1 handler transactions than the provider can provide.
    // Expecting the following behavior:
    // 1. Request 10 l1 handler transactions, get 5 (no more l1 handler txs from provider). Request
    //    5 more from mempool.
    // 2. Request 10 mempool transactions.
    const NUM_L1_HANDLER_TXS_IN_PROVIDER: usize = 5;

    mock_dependencies.expect_get_l1_handler_txs(MAX_TXS_PER_FETCH, NUM_L1_HANDLER_TXS_IN_PROVIDER);
    let n_mempool_left = MAX_TXS_PER_FETCH - NUM_L1_HANDLER_TXS_IN_PROVIDER;
    mock_dependencies.expect_get_mempool_txs(n_mempool_left);
    mock_dependencies.expect_get_mempool_txs(MAX_TXS_PER_FETCH);

    let mut tx_provider = mock_dependencies.propose_tx_provider();

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(
        data[..NUM_L1_HANDLER_TXS_IN_PROVIDER]
            .iter()
            .all(|tx| matches!(tx, InternalConsensusTransaction::L1Handler(_)))
    );
    assert!(
        data[NUM_L1_HANDLER_TXS_IN_PROVIDER..]
            .iter()
            .all(|tx| { matches!(tx, InternalConsensusTransaction::RpcTransaction(_)) })
    );

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, InternalConsensusTransaction::RpcTransaction(_))));
}

#[rstest]
#[tokio::test]
async fn validate_flow(mut mock_dependencies: MockDependencies) {
    let test_tx = test_l1handler_tx();
    mock_dependencies.expect_validate_l1handler(test_tx.clone(), L1ValidationStatus::Validated);
    mock_dependencies
        .simulate_input_txs(vec![
            InternalConsensusTransaction::L1Handler(test_tx),
            InternalConsensusTransaction::RpcTransaction(internal_invoke_tx(
                InvokeTxArgs::default(),
            )),
        ])
        .await;
    let mut validate_tx_provider = mock_dependencies.validate_tx_provider();

    let txs = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, txs => txs);
    assert_eq!(data.len(), 2);
    assert!(matches!(data[0], InternalConsensusTransaction::L1Handler(_)));
    assert!(matches!(data[1], InternalConsensusTransaction::RpcTransaction(_)));
}

#[rstest]
#[tokio::test]
async fn get_final_n_executed_txs(mock_dependencies: MockDependencies) {
    let (mut validate_tx_provider, final_n_executed_txs_sender) =
        mock_dependencies.validate_tx_provider_with_final_n_executed_txs();

    // Calling `get_final_n_executed_txs` before sending the number of transactions returns `None`.
    assert_eq!(validate_tx_provider.get_final_n_executed_txs().await, None);

    // Send the number of transactions and verify that it is returned.
    final_n_executed_txs_sender.send(10).unwrap();
    assert_eq!(validate_tx_provider.get_final_n_executed_txs().await, Some(10));

    // Future calls to `get_final_n_executed_txsed_txs` return `None`.
    assert_eq!(validate_tx_provider.get_final_n_executed_txs().await, None);
}

#[rstest]
#[tokio::test]
async fn validate_fails(
    mut mock_dependencies: MockDependencies,
    #[values(
        InvalidValidationStatus::AlreadyIncludedInProposedBlock,
        InvalidValidationStatus::AlreadyIncludedOnL2,
        InvalidValidationStatus::ConsumedOnL1,
        InvalidValidationStatus::NotFound
    )]
    expected_validation_status: InvalidValidationStatus,
) {
    let test_tx = test_l1handler_tx();
    mock_dependencies.expect_validate_l1handler(
        test_tx.clone(),
        L1ValidationStatus::Invalid(expected_validation_status),
    );
    mock_dependencies
        .simulate_input_txs(vec![
            InternalConsensusTransaction::L1Handler(test_tx),
            InternalConsensusTransaction::RpcTransaction(internal_invoke_tx(
                InvokeTxArgs::default(),
            )),
        ])
        .await;
    let mut validate_tx_provider = mock_dependencies.validate_tx_provider();

    let result = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await;
    assert_matches!(
        result,
        Err(TransactionProviderError::L1HandlerTransactionValidationFailed { validation_status, .. })
        if validation_status == expected_validation_status
    );
}
