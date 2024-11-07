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
    TransactionProviderConfig,
};

const MAX_L1_HANDLER_TXS_PER_BLOCK: usize = 15;
const MAX_TXS_PER_FETCH: usize = 10;

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

    fn propose_tx_provider(self) -> ProposeTransactionProvider {
        ProposeTransactionProvider::new(
            TransactionProviderConfig {
                max_l1_handler_txs_per_block: MAX_L1_HANDLER_TXS_PER_BLOCK,
            },
            Arc::new(self.mempool_client),
            Arc::new(self.l1_provider_client),
        )
    }
}

#[fixture]
fn mock_dependencies() -> MockDependencies {
    MockDependencies {
        mempool_client: MockMempoolClient::new(),
        l1_provider_client: MockL1ProviderClient::new(),
    }
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
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == 10 => txs);
    assert!(data[..5].iter().all(|tx| matches!(tx, Transaction::L1Handler(_))));
    assert!(data[5..].iter().all(|tx| matches!(tx, Transaction::Account(_))));

    let txs = tx_provider.get_txs(MAX_TXS_PER_FETCH).await.unwrap();
    let data = assert_matches!(txs, NextTxs::Txs(txs) if txs.len() == 10 => txs);
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
