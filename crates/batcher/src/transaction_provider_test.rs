use std::sync::Arc;

use assert_matches::assert_matches;
use mockall::predicate::eq;
use rstest::{fixture, rstest};
use starknet_api::executable_transaction::{AccountTransaction, L1HandlerTransaction, Transaction};
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_mempool_types::communication::MockMempoolClient;

use crate::transaction_provider::{
    MockL1ProviderClient,
    NextTxs,
    ProposeTransactionProvider,
    TransactionProvider,
    TransactionProviderError,
    ValidateTransactionProvider,
};

const MAX_L1_HANDLER_TXS_PER_BLOCK: usize = 15;
const MAX_TXS_PER_FETCH: usize = 10;
const VALIDATE_BUFFER_SIZE: usize = 30;

struct MockDependencies {
    mempool_client: MockMempoolClient,
    l1_provider_client: MockL1ProviderClient,
}

impl MockDependencies {
    fn expect_get_l1_handler_txs(&mut self, n_to_request: usize, n_to_return: usize) {
        self.l1_provider_client
            .expect_get_txs()
            .with(eq(n_to_request))
            .returning(move |_| vec![L1HandlerTransaction::default(); n_to_return]);
    }

    fn expect_get_mempool_txs(&mut self, n_to_request: usize) {
        self.mempool_client.expect_get_txs().with(eq(n_to_request)).returning(move |n_requested| {
            Ok(vec![
                AccountTransaction::Invoke(executable_invoke_tx(InvokeTxArgs::default()));
                n_requested
            ])
        });
    }

    fn expect_validate_l1handler(&mut self, tx: L1HandlerTransaction, result: bool) {
        self.l1_provider_client
            .expect_validate()
            .withf(move |tx_arg| tx_arg == &tx)
            .returning(move |_| result);
    }

    fn propose_tx_provider(self) -> ProposeTransactionProvider {
        ProposeTransactionProvider::new(
            Arc::new(self.mempool_client),
            Arc::new(self.l1_provider_client),
            MAX_L1_HANDLER_TXS_PER_BLOCK,
        )
    }

    fn validate_tx_provider(
        self,
        tx_receiver: tokio::sync::mpsc::Receiver<Transaction>,
    ) -> ValidateTransactionProvider {
        ValidateTransactionProvider {
            tx_receiver,
            l1_provider_client: Arc::new(self.l1_provider_client),
        }
    }
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    MockDependencies {
        mempool_client: MockMempoolClient::new(),
        l1_provider_client: MockL1ProviderClient::new(),
    }
}

#[fixture]
fn tx_channel() -> (tokio::sync::mpsc::Sender<Transaction>, tokio::sync::mpsc::Receiver<Transaction>)
{
    tokio::sync::mpsc::channel(VALIDATE_BUFFER_SIZE)
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
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, Transaction::L1Handler(_))));

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data[..n_l1handler_left].iter().all(|tx| matches!(tx, Transaction::L1Handler(_))));
    assert!(data[n_l1handler_left..].iter().all(|tx| matches!(tx, Transaction::Account(_))));

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, Transaction::Account(_))));
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
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(
        data[..NUM_L1_HANDLER_TXS_IN_PROVIDER]
            .iter()
            .all(|tx| matches!(tx, Transaction::L1Handler(_)))
    );
    assert!(
        data[NUM_L1_HANDLER_TXS_IN_PROVIDER..]
            .iter()
            .all(|tx| { matches!(tx, Transaction::Account(_)) })
    );

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == MAX_TXS_PER_FETCH => txs);
    assert!(data.iter().all(|tx| matches!(tx, Transaction::Account(_))));
}

#[rstest]
#[tokio::test]
async fn validate_flow(
    mut mock_dependencies: MockDependencies,
    tx_channel: (tokio::sync::mpsc::Sender<Transaction>, tokio::sync::mpsc::Receiver<Transaction>),
) {
    mock_dependencies.expect_validate_l1handler(L1HandlerTransaction::default(), true);
    let (tx_sender, tx_receiver) = tx_channel;
    let mut validate_tx_provider = mock_dependencies.validate_tx_provider(tx_receiver);

    tx_sender.send(Transaction::L1Handler(L1HandlerTransaction::default())).await.unwrap();
    tx_sender
        .send(Transaction::Account(AccountTransaction::Invoke(executable_invoke_tx(
            InvokeTxArgs::default(),
        ))))
        .await
        .unwrap();

    let txs = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, NextTxs::Txs(txs) => txs);
    assert_eq!(data.len(), 2);
    assert!(matches!(data[0], Transaction::L1Handler(_)));
    assert!(matches!(data[1], Transaction::Account(_)));
}

#[rstest]
#[tokio::test]
async fn validate_fails(
    mut mock_dependencies: MockDependencies,
    tx_channel: (tokio::sync::mpsc::Sender<Transaction>, tokio::sync::mpsc::Receiver<Transaction>),
) {
    mock_dependencies.expect_validate_l1handler(L1HandlerTransaction::default(), false);
    let (tx_sender, tx_receiver) = tx_channel;
    let mut validate_tx_provider = mock_dependencies.validate_tx_provider(tx_receiver);

    tx_sender.send(Transaction::L1Handler(L1HandlerTransaction::default())).await.unwrap();
    tx_sender
        .send(Transaction::Account(AccountTransaction::Invoke(executable_invoke_tx(
            InvokeTxArgs::default(),
        ))))
        .await
        .unwrap();

    let result = validate_tx_provider.get_txs(MAX_TXS_PER_FETCH).await;
    assert_matches!(
        result,
        Err(TransactionProviderError::L1HandlerTransactionValidationFailed(_tx_hash))
    );
}
