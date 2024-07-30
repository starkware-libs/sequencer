use std::borrow::BorrowMut;
use std::sync::Arc;

use rstest::{fixture, rstest};
use starknet_mempool_types::communication::{MempoolClientResult, MockMempoolClient};
use starknet_mempool_types::mempool_types::ThinTransaction;

use crate::proposer::{Proposer, ProposerConfig, ProposerError};

const MAX_N_TXS_TO_FETCH: usize = 2;

#[fixture]
fn proposer_config() -> ProposerConfig {
    ProposerConfig { n_txs_to_fetch: MAX_N_TXS_TO_FETCH }
}

#[rstest]
#[case::mempool_empty(Ok(vec![]))]
#[case::mempool_few_txs(Ok(vec![ThinTransaction::default()]))]
#[case::mempool_full(Ok(vec![ThinTransaction::default(); MAX_N_TXS_TO_FETCH]))]
#[tokio::test]
async fn test_take_more_transactions(
    #[case] mempool_response: MempoolClientResult<Vec<ThinTransaction>>,
    proposer_config: ProposerConfig,
) {
    let mempool_response_clone = mempool_response.clone();
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .borrow_mut()
        .expect_get_txs()
        .withf(move |n_txs| *n_txs == proposer_config.n_txs_to_fetch)
        .return_once(move |_| mempool_response_clone);

    let proposer =
        Proposer { mempool_client: Arc::new(mock_mempool_client), config: proposer_config };

    let result = proposer.take_more_transactions().await;
    let expected_result = mempool_response.map_err(ProposerError::MempoolClientError);

    // The error type is not comparable, so we compare the debug representation instead.
    assert_eq!(format!("{result:?}"), format!("{expected_result:?}"));
}
