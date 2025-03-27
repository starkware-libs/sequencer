use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_test_utils::{get_rng, GetTestInstance};
use mempool_test_utils::starknet_api_test_utils::test_valid_resource_bounds;
use metrics_exporter_prometheus::PrometheusBuilder;
use mockall::predicate;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{GasPrice, NonzeroGasPrice};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::test_utils::declare::{internal_rpc_declare_tx, DeclareTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_api::{contract_address, declare_tx_args, nonce, tx_hash};
use starknet_mempool_p2p_types::communication::MockMempoolP2pPropagatorClient;
use starknet_mempool_types::communication::AddTransactionArgsWrapper;
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};
use starknet_sequencer_metrics::metrics::HistogramValue;

use crate::communication::MempoolCommunicationWrapper;
use crate::mempool::{Mempool, MempoolConfig, MempoolContent, MempoolState, TransactionReference};
use crate::metrics::register_metrics;
use crate::test_utils::{
    add_tx,
    add_tx_expect_error,
    commit_block,
    get_txs_and_assert_expected,
    FakeClock,
    MempoolMetrics,
};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;
use crate::{add_tx_input, tx};

// Utils.

/// Represents the internal content of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct MempoolTestContent {
    pub tx_pool: Option<HashMap<TransactionHash, InternalRpcTransaction>>,
    pub priority_txs: Option<Vec<TransactionReference>>,
    pub pending_txs: Option<Vec<TransactionReference>>,
}

impl MempoolTestContent {
    #[track_caller]
    fn assert_eq(&self, mempool_content: &MempoolContent) {
        if let Some(tx_pool) = &self.tx_pool {
            assert_eq!(&mempool_content.tx_pool, tx_pool);
        }

        if let Some(priority_txs) = &self.priority_txs {
            assert_eq!(&mempool_content.priority_txs, priority_txs);
        }

        if let Some(pending_txs) = &self.pending_txs {
            assert_eq!(&mempool_content.pending_txs, pending_txs);
        }
    }
}

#[derive(Debug)]
struct MempoolTestContentBuilder {
    config: MempoolConfig,
    content: MempoolTestContent,
    gas_price_threshold: NonzeroGasPrice,
}

impl MempoolTestContentBuilder {
    fn new() -> Self {
        Self {
            config: MempoolConfig { enable_fee_escalation: false, ..Default::default() },
            content: MempoolTestContent::default(),
            gas_price_threshold: NonzeroGasPrice::default(),
        }
    }

    fn with_pool<P>(mut self, pool_txs: P) -> Self
    where
        P: IntoIterator<Item = InternalRpcTransaction>,
    {
        self.content.tx_pool = Some(pool_txs.into_iter().map(|tx| (tx.tx_hash, tx)).collect());
        self
    }

    fn with_priority_queue<Q>(mut self, queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        self.content.priority_txs = Some(queue_txs.into_iter().collect());
        self
    }

    fn with_pending_queue<Q>(mut self, queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        self.content.pending_txs = Some(queue_txs.into_iter().collect());
        self
    }

    fn with_gas_price_threshold(mut self, gas_price_threshold: u128) -> Self {
        self.gas_price_threshold = NonzeroGasPrice::new_unchecked(gas_price_threshold.into());
        self
    }

    fn with_fee_escalation_percentage(mut self, fee_escalation_percentage: u8) -> Self {
        self.config = MempoolConfig {
            enable_fee_escalation: true,
            fee_escalation_percentage,
            ..Default::default()
        };
        self
    }

    fn build(self) -> MempoolTestContent {
        self.content
    }

    fn build_full_mempool(self) -> Mempool {
        Mempool {
            config: self.config.clone(),
            delayed_declares: VecDeque::new(),
            tx_pool: self.content.tx_pool.unwrap_or_default().into_values().collect(),
            tx_queue: TransactionQueue::new(
                self.content.priority_txs.unwrap_or_default(),
                self.content.pending_txs.unwrap_or_default(),
                self.gas_price_threshold,
            ),
            state: MempoolState::new(self.config.committed_nonce_retention_block_count),
            clock: Arc::new(FakeClock::default()),
        }
    }
}

impl FromIterator<InternalRpcTransaction> for TransactionPool {
    fn from_iter<T: IntoIterator<Item = InternalRpcTransaction>>(txs: T) -> Self {
        let mut pool = Self::new(Arc::new(FakeClock::default()));
        for tx in txs {
            pool.insert(tx).unwrap();
        }
        pool
    }
}

fn declare_add_tx_input(args: DeclareTxArgs) -> AddTransactionArgs {
    let tx = internal_rpc_declare_tx(args);
    let account_state = AccountState { address: tx.contract_address(), nonce: tx.nonce() };

    AddTransactionArgs { tx, account_state }
}

#[track_caller]
fn builder_with_queue(
    in_priority_queue: bool,
    in_pending_queue: bool,
    tx: &InternalRpcTransaction,
) -> MempoolTestContentBuilder {
    assert!(
        !(in_priority_queue && in_pending_queue),
        "A transaction can be in at most one queue at a time."
    );

    let mut builder = MempoolTestContentBuilder::new();

    if in_priority_queue {
        builder = builder.with_priority_queue([TransactionReference::new(tx)]);
    }

    if in_pending_queue {
        builder = builder.with_pending_queue([TransactionReference::new(tx)]);
    }

    builder
}

#[track_caller]
fn add_tx_and_verify_replacement(
    mut mempool: Mempool,
    valid_replacement_input: AddTransactionArgs,
    in_priority_queue: bool,
    in_pending_queue: bool,
) {
    add_tx(&mut mempool, &valid_replacement_input);

    // Verify transaction was replaced.
    let builder =
        builder_with_queue(in_priority_queue, in_pending_queue, &valid_replacement_input.tx);

    let expected_mempool_content = builder.with_pool([valid_replacement_input.tx]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[track_caller]
fn add_tx_and_verify_replacement_in_pool(
    mempool: Mempool,
    valid_replacement_input: AddTransactionArgs,
) {
    let in_priority_queue = false;
    let in_pending_queue = false;
    add_tx_and_verify_replacement(
        mempool,
        valid_replacement_input,
        in_priority_queue,
        in_pending_queue,
    );
}

#[track_caller]
fn add_txs_and_verify_no_replacement(
    mut mempool: Mempool,
    existing_tx: InternalRpcTransaction,
    invalid_replacement_inputs: impl IntoIterator<Item = AddTransactionArgs>,
    in_priority_queue: bool,
    in_pending_queue: bool,
) {
    for input in invalid_replacement_inputs {
        add_tx_expect_error(
            &mut mempool,
            &input,
            MempoolError::DuplicateNonce {
                address: input.tx.contract_address(),
                nonce: input.tx.nonce(),
            },
        );
    }

    // Verify transaction was not replaced.
    let builder = builder_with_queue(in_priority_queue, in_pending_queue, &existing_tx);

    let expected_mempool_content = builder.with_pool([existing_tx]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[track_caller]
fn add_txs_and_verify_no_replacement_in_pool(
    mempool: Mempool,
    existing_tx: InternalRpcTransaction,
    invalid_replacement_inputs: impl IntoIterator<Item = AddTransactionArgs>,
) {
    let in_priority_queue = false;
    let in_pending_queue = false;
    add_txs_and_verify_no_replacement(
        mempool,
        existing_tx,
        invalid_replacement_inputs,
        in_priority_queue,
        in_pending_queue,
    );
}

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    MempoolTestContentBuilder::new().build_full_mempool()
}

// Tests.

// `get_txs` tests.

#[rstest]
#[case::test_get_zero_txs(0)]
#[case::test_get_exactly_all_eligible_txs(3)]
#[case::test_get_more_than_all_eligible_txs(5)]
#[case::test_get_less_than_all_eligible_txs(2)]
fn test_get_txs_returns_by_priority(#[case] n_requested_txs: usize) {
    // Setup.
    let tx_tip_20 = tx!(tx_hash: 1, address: "0x0", tip: 20);
    let tx_tip_30 = tx!(tx_hash: 2, address: "0x1", tip: 30);
    let tx_tip_10 = tx!(tx_hash: 3, address: "0x2", tip: 10);

    let queue_txs = [&tx_tip_20, &tx_tip_30, &tx_tip_10].map(TransactionReference::new);
    let pool_txs = [&tx_tip_20, &tx_tip_30, &tx_tip_10].map(|tx| tx.clone());
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_full_mempool();

    // Test.
    let fetched_txs = mempool.get_txs(n_requested_txs).unwrap();

    // Check that the returned transactions are the ones with the highest priority.
    let sorted_txs = [tx_tip_30, tx_tip_20, tx_tip_10];
    let (expected_fetched_txs, remaining_txs) = sorted_txs.split_at(fetched_txs.len());
    assert_eq!(fetched_txs, expected_fetched_txs);

    // Assert: non-returned transactions are still in the mempool.
    let remaining_tx_references = remaining_txs.iter().map(TransactionReference::new);
    let expected_mempool_content =
        MempoolTestContentBuilder::new().with_priority_queue(remaining_tx_references).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn test_get_txs_returns_by_secondary_priority_on_tie() {
    // Setup.
    let tx_tip_10_hash_9 = tx!(tx_hash: 9, address: "0x2", tip: 10);
    let tx_tip_10_hash_15 = tx!(tx_hash: 15, address: "0x0", tip: 10);

    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool([&tx_tip_10_hash_9, &tx_tip_10_hash_15].map(|tx| tx.clone()))
        .with_priority_queue([&tx_tip_10_hash_9, &tx_tip_10_hash_15].map(TransactionReference::new))
        .build_full_mempool();

    // Test and assert.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_tip_10_hash_15, tx_tip_10_hash_9]);
}

#[rstest]
fn test_get_txs_does_not_return_pending_txs() {
    // Setup.
    let tx = tx!();

    let mut mempool = MempoolTestContentBuilder::new()
        .with_pending_queue([TransactionReference::new(&tx)])
        .with_pool([tx])
        .build_full_mempool();

    // Test and assert.
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

#[rstest]
fn test_get_txs_does_not_remove_returned_txs_from_pool() {
    // Setup.
    let tx = tx!();

    let queue_txs = [TransactionReference::new(&tx)];
    let pool_txs = [tx];
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_full_mempool();

    // Test and assert: all transactions are returned.
    get_txs_and_assert_expected(&mut mempool, 2, &pool_txs);
    let expected_mempool_content =
        MempoolTestContentBuilder::new().with_pool(pool_txs).with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn test_get_txs_replenishes_queue_only_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 = tx!(tx_hash: 1, address: "0x0", tx_nonce: 0, tip: 20);
    let tx_address_0_nonce_1 = tx!(tx_hash: 2, address: "0x0", tx_nonce: 1, tip: 20);
    let tx_address_1_nonce_0 = tx!(tx_hash: 3, address: "0x1", tx_nonce: 0, tip: 10);

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs =
        [&tx_address_0_nonce_0, &tx_address_0_nonce_1, &tx_address_1_nonce_0].map(|tx| tx.clone());
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_full_mempool();

    // Test and assert: all transactions returned.
    // Replenishment done in chunks: account 1 transaction is returned before the one of account 0,
    // although its priority is higher.
    get_txs_and_assert_expected(
        &mut mempool,
        3,
        &[tx_address_0_nonce_0, tx_address_1_nonce_0, tx_address_0_nonce_1],
    );
    let expected_mempool_content = MempoolTestContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn test_get_txs_with_nonce_gap() {
    // Setup.
    let tx_address_0_nonce_1 = tx!(tx_hash: 2, address: "0x0", tx_nonce: 1);
    let tx_address_1_nonce_0 = tx!(tx_hash: 3, address: "0x1", tx_nonce: 0);

    let queue_txs = [TransactionReference::new(&tx_address_1_nonce_0)];
    let pool_txs = [tx_address_0_nonce_1, tx_address_1_nonce_0.clone()];
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_full_mempool();

    // Test and assert.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_address_1_nonce_0]);
    let expected_mempool_content = MempoolTestContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

// `add_tx` tests.

#[rstest]
fn test_add_tx_insertion_sorted_by_priority(mut mempool: Mempool) {
    // Setup.
    let input_tip_50 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0, tip: 50);
    // The following transactions test a scenario with a higher tip and lower hash, covering
    // both primary and secondary priority.
    let input_tip_100 =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 1, account_nonce: 1, tip: 100);
    let input_tip_80 =
        add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 2, account_nonce: 2, tip: 80);

    // Test.
    for input in [&input_tip_50, &input_tip_100, &input_tip_80] {
        add_tx(&mut mempool, input);
    }

    // Assert: transactions are ordered by priority.
    let expected_queue_txs =
        [&input_tip_100.tx, &input_tip_80.tx, &input_tip_50.tx].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolTestContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn test_add_tx_correctly_places_txs_in_queue_and_pool(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 3, address: "0x0", tx_nonce: 1, account_nonce: 0);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 0,account_nonce: 0);

    // Test.
    for input in [&input_address_0_nonce_0, &input_address_1_nonce_0, &input_address_0_nonce_1] {
        add_tx(&mut mempool, input);
    }

    // Assert: only the eligible transactions appear in the queue.
    let expected_queue_txs =
        [&input_address_1_nonce_0.tx, &input_address_0_nonce_0.tx].map(TransactionReference::new);
    let expected_pool_txs =
        [input_address_0_nonce_0.tx, input_address_1_nonce_0.tx, input_address_0_nonce_1.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool.content());
}

// TODO(Elin): reconsider this test in a more realistic scenario.
#[rstest]
fn test_add_tx_rejects_duplicate_tx_hash(mut mempool: Mempool) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    // Setup.
    let input = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);
    // Same hash is possible if signature is different, for example.
    // This is an artificially crafted transaction with a different nonce in order to skip
    // replacement logic.
    let duplicate_input = add_tx_input!(tx_hash: 1, tx_nonce: 2, account_nonce: 0);

    // Test.
    add_tx(&mut mempool, &input);
    add_tx_expect_error(
        &mut mempool,
        &duplicate_input,
        MempoolError::DuplicateTransaction { tx_hash: input.tx.tx_hash() },
    );

    // Assert: the original transaction remains.
    let expected_mempool_content = MempoolTestContentBuilder::new().with_pool([input.tx]).build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Assert: metrics.
    let expected_metrics = MempoolMetrics {
        txs_received_invoke: 2,
        txs_dropped_failed_add_tx_checks: 1,
        pool_size: 1,
        ..Default::default()
    };
    expected_metrics.verify_metrics(&recorder);
}

#[rstest]
#[case::lower_nonce(0, MempoolError::NonceTooOld { address: contract_address!("0x0"), nonce: nonce!(0) })]
#[case::equal_nonce(1, MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(1) })]
fn test_add_tx_rejects_tx_of_queued_nonce(
    #[case] tx_nonce: u64,
    #[case] expected_error: MempoolError,
    mut mempool: Mempool,
) {
    // Setup.
    let input = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 1, account_nonce: 1);
    add_tx(&mut mempool, &input);

    // Test and assert: original transaction remains.
    let invalid_input =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: tx_nonce, account_nonce: 1);
    add_tx_expect_error(&mut mempool, &invalid_input, expected_error);
}

#[rstest]
fn test_add_tx_with_identical_tip_succeeds(mut mempool: Mempool) {
    // Setup.
    let input1 = add_tx_input!(tx_hash: 2, address: "0x0", tip: 1);
    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tx_hash: 1, address: "0x1", tip: 1);

    // Test.
    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Assert: both transactions are in the mempool.
    let expected_queue_txs = [&input1.tx, &input2.tx].map(TransactionReference::new);
    let expected_pool_txs = [input1.tx, input2.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();

    // TODO(AlonH): currently hash comparison tie-breaks the two. Once more robust tie-breaks are
    // added replace this assertion with a dedicated test.
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn add_tx_with_committed_account_nonce(mut mempool: Mempool) {
    // Setup: commit a block with account nonce 1.
    commit_block(&mut mempool, [("0x0", 1)], []);

    // Add a transaction with nonce 0. Should be rejected with NonceTooOld.
    let input = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    add_tx_expect_error(
        &mut mempool,
        &input,
        MempoolError::NonceTooOld { address: contract_address!("0x0"), nonce: nonce!(0) },
    );

    // Add a transaction with nonce 1. Should be accepted.
    let input = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, account_nonce: 0);
    add_tx(&mut mempool, &input);
}

#[rstest]
fn test_add_tx_fills_nonce_gap(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 1, tx_nonce: 0, account_nonce: 0);
    let input_nonce_1 = add_tx_input!(tx_hash: 2, tx_nonce: 1, account_nonce: 0);

    // Test: add the second transaction first, which creates a hole in the sequence.
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: the second transaction is in the pool and not in the queue.
    let expected_pool_txs = [input_nonce_1.tx.clone()];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue([])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool.content());
}

// `commit_block` tests.

#[rstest]
fn test_commit_block_includes_all_proposed_txs() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tx_hash: 1, address: "0x0", tx_nonce: 3);
    let tx_address_0_nonce_4 = tx!(tx_hash: 2, address: "0x0", tx_nonce: 4);
    let tx_address_0_nonce_5 = tx!(tx_hash: 3, address: "0x0", tx_nonce: 5);
    let tx_address_1_nonce_2 = tx!(tx_hash: 4, address: "0x1", tx_nonce: 2);
    let tx_address_1_nonce_3 = tx!(tx_hash: 5, address: "0x1", tx_nonce: 3);
    let tx_address_2_nonce_1 = tx!(tx_hash: 6, address: "0x2", tx_nonce: 1);

    let queue_txs = [&tx_address_2_nonce_1, &tx_address_1_nonce_3, &tx_address_0_nonce_4]
        .map(TransactionReference::new);
    let pool_txs = [
        tx_address_0_nonce_3,
        tx_address_0_nonce_4.clone(),
        tx_address_0_nonce_5.clone(),
        tx_address_1_nonce_2,
        tx_address_1_nonce_3.clone(),
        tx_address_2_nonce_1.clone(),
    ];
    let mut mempool = MempoolTestContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_full_mempool();

    // Test.
    let nonces = [("0x0", 4), ("0x1", 3)];
    commit_block(&mut mempool, nonces, []);

    // Assert.
    let pool_txs =
        [tx_address_0_nonce_4, tx_address_0_nonce_5, tx_address_1_nonce_3, tx_address_2_nonce_1];
    let expected_mempool_content =
        MempoolTestContentBuilder::new().with_pool(pool_txs).with_priority_queue(queue_txs).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

// Fee escalation tests.

#[rstest]
#[case::pool(false, false)]
#[case::pool_and_priority_queue(true, false)]
#[case::pool_and_pending_queue(false, true)]
fn test_fee_escalation_valid_replacement(
    #[case] in_priority_queue: bool,
    #[case] in_pending_queue: bool,
) {
    let increased_values = [
        99,  // Exactly increase percentage.
        100, // More than increase percentage,
        180, // More than 100% increase, to check percentage calculation.
    ];
    for increased_value in increased_values {
        // Setup.
        let tx = tx!(tx_hash: 0, tip: 90, max_l2_gas_price: 90);

        let mut builder = builder_with_queue(in_priority_queue, in_pending_queue, &tx)
            .with_fee_escalation_percentage(10);

        if in_pending_queue {
            builder = builder.with_gas_price_threshold(1000);
        }

        let mempool = builder.with_pool([tx]).build_full_mempool();

        let valid_replacement_input = add_tx_input!(tx_hash: 1, tip: increased_value, max_l2_gas_price: u128::from(increased_value));

        // Test and assert.
        add_tx_and_verify_replacement(
            mempool,
            valid_replacement_input,
            in_priority_queue,
            in_pending_queue,
        );
    }
}

#[rstest]
#[case::pool(false, false)]
#[case::pool_and_priority_queue(true, false)]
#[case::pool_and_pending_queue(false, true)]
fn test_fee_escalation_invalid_replacement(
    #[case] in_priority_queue: bool,
    #[case] in_pending_queue: bool,
) {
    // Setup.
    let existing_tx = tx!(tx_hash: 1, tip: 100, max_l2_gas_price: 100);

    let mut builder = builder_with_queue(in_priority_queue, in_pending_queue, &existing_tx)
        .with_fee_escalation_percentage(10);

    if in_pending_queue {
        builder = builder.with_gas_price_threshold(1000);
    }

    let mempool = builder.with_pool([existing_tx.clone()]).build_full_mempool();

    let input_not_enough_tip = add_tx_input!(tx_hash: 3, tip: 109, max_l2_gas_price: 110);
    let input_not_enough_gas_price = add_tx_input!(tx_hash: 4, tip: 110, max_l2_gas_price: 109);
    let input_not_enough_both = add_tx_input!(tx_hash: 5, tip: 109, max_l2_gas_price: 109);

    // Test and assert.
    let invalid_replacement_inputs =
        [input_not_enough_tip, input_not_enough_gas_price, input_not_enough_both];
    add_txs_and_verify_no_replacement(
        mempool,
        existing_tx,
        invalid_replacement_inputs,
        in_priority_queue,
        in_pending_queue,
    );
}

#[rstest]
fn fee_escalation_queue_removal() {
    // Setup.
    let min_gas_price = 1;
    let queued_tx =
        tx!(tx_hash: 0, address: "0x0", tx_nonce: 0, tip: 0, max_l2_gas_price: min_gas_price);
    let queued_tx_reference = TransactionReference::new(&queued_tx);
    let tx_to_be_replaced =
        tx!(tx_hash: 1, address: "0x0", tx_nonce: 1, tip: 0, max_l2_gas_price: min_gas_price);
    let mut mempool = MempoolTestContentBuilder::new()
        .with_priority_queue([queued_tx_reference])
        .with_pool([queued_tx.clone(), tx_to_be_replaced])
        .with_fee_escalation_percentage(0) // Always replace.
        .build_full_mempool();

    // Test and assert: replacement doesn't affect queue.

    let valid_replacement_input = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, tip: 0, max_l2_gas_price: min_gas_price);
    add_tx(&mut mempool, &valid_replacement_input);
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool([queued_tx, valid_replacement_input.tx])
        .with_priority_queue([queued_tx_reference])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn test_fee_escalation_valid_replacement_minimum_values() {
    // Setup.
    let min_gas_price = 1;
    let tx = tx!(tx_hash: 0, tip: 0, max_l2_gas_price: min_gas_price);
    let mempool = MempoolTestContentBuilder::new()
        .with_pool([tx])
        .with_fee_escalation_percentage(0) // Always replace.
        .build_full_mempool();

    // Test and assert: replacement with maximum values.
    let valid_replacement_input =
        add_tx_input!(tx_hash: 1, tip: 0, max_l2_gas_price: min_gas_price);
    add_tx_and_verify_replacement_in_pool(mempool, valid_replacement_input);
}

#[rstest]
fn test_fee_escalation_valid_replacement_maximum_values() {
    // Setup.
    let tx = tx!(tx_hash: 0, tip: u64::MAX / 100, max_l2_gas_price: u128::MAX / 100 );
    let mempool = MempoolTestContentBuilder::new()
        .with_pool([tx])
        .with_fee_escalation_percentage(100)
        .build_full_mempool();

    // Test and assert: replacement with maximum values.
    let valid_replacement_input =
        add_tx_input!(tx_hash: 1, tip: u64::MAX / 50 , max_l2_gas_price: u128::MAX / 50);
    add_tx_and_verify_replacement_in_pool(mempool, valid_replacement_input);
}

#[rstest]
fn test_fee_escalation_invalid_replacement_overflow_gracefully_handled() {
    // Initial transaction with high values.

    // Setup.
    let initial_values = [
        (u64::MAX - 10, 10),
        (u64::MAX, 10),
        (10, u128::MAX - 10),
        (10, u128::MAX),
        (u64::MAX - 10, u128::MAX - 10),
        (u64::MAX, u128::MAX),
    ];
    for (tip, max_l2_gas_price) in initial_values {
        let existing_tx = tx!(tx_hash: 0, tip: tip, max_l2_gas_price: max_l2_gas_price);
        let mempool = MempoolTestContentBuilder::new()
            .with_pool([existing_tx.clone()])
            .with_fee_escalation_percentage(10)
            .build_full_mempool();

        // Test and assert: overflow gracefully handled.
        let invalid_replacement_input =
            add_tx_input!(tx_hash: 1, tip: u64::MAX, max_l2_gas_price: u128::MAX);
        add_txs_and_verify_no_replacement_in_pool(
            mempool,
            existing_tx,
            [invalid_replacement_input],
        );
    }

    // Large percentage.

    // Setup.
    let existing_tx = tx!(tx_hash: 0, tip: u64::MAX >> 1, max_l2_gas_price: u128::MAX >> 1);
    let mempool = MempoolTestContentBuilder::new()
        .with_pool([existing_tx.clone()])
        .with_fee_escalation_percentage(200)
        .build_full_mempool();

    // Test and assert: overflow gracefully handled.
    let invalid_replacement_input =
        add_tx_input!(tx_hash: 1, tip: u64::MAX, max_l2_gas_price: u128::MAX);
    add_txs_and_verify_no_replacement_in_pool(mempool, existing_tx, [invalid_replacement_input]);
}

// `update_gas_price_threshold` tests.

#[rstest]
fn test_update_gas_price_threshold_increases_threshold() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    // Setup.
    let [tx_low_gas, tx_high_gas] = [
        &tx!(tx_hash: 0, address: "0x0", max_l2_gas_price: 100),
        &tx!(tx_hash: 1, address: "0x1", max_l2_gas_price: 101),
    ]
    .map(TransactionReference::new);

    let mut mempool: Mempool = MempoolTestContentBuilder::new()
        .with_priority_queue([tx_low_gas, tx_high_gas])
        .with_gas_price_threshold(100)
        .build_full_mempool();

    // Test.
    mempool.update_gas_price(NonzeroGasPrice::new_unchecked(GasPrice(101)));

    // Assert.
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pending_queue([tx_low_gas])
        .with_priority_queue([tx_high_gas])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Assert: metrics.
    let expected_metrics =
        MempoolMetrics { priority_queue_size: 1, pending_queue_size: 1, ..Default::default() };
    expected_metrics.verify_metrics(&recorder);
}

#[rstest]
fn test_update_gas_price_threshold_decreases_threshold() {
    // Setup.
    let [tx_low_gas, tx_high_gas] = [
        &tx!(tx_hash: 0, address: "0x0", max_l2_gas_price: 89),
        &tx!(tx_hash: 1, address: "0x1", max_l2_gas_price: 90),
    ]
    .map(TransactionReference::new);

    let mut mempool: Mempool = MempoolTestContentBuilder::new()
        .with_pending_queue([tx_low_gas, tx_high_gas])
        .with_gas_price_threshold(100)
        .build_full_mempool();

    // Test.
    mempool.update_gas_price(NonzeroGasPrice::new_unchecked(GasPrice(90)));

    // Assert.
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pending_queue([tx_low_gas])
        .with_priority_queue([tx_high_gas])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
#[tokio::test]
async fn test_new_tx_sent_to_p2p(mempool: Mempool) {
    // add_tx_input! creates an Invoke Transaction
    let tx_args = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 2, account_nonce: 2);
    let propagateor_args =
        AddTransactionArgsWrapper { args: tx_args.clone(), p2p_message_metadata: None };
    let mut mock_mempool_p2p_propagator_client = MockMempoolP2pPropagatorClient::new();
    mock_mempool_p2p_propagator_client
        .expect_add_transaction()
        .times(1)
        .with(predicate::eq(tx_args.tx))
        .returning(|_| Ok(()));
    let mut mempool_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(mock_mempool_p2p_propagator_client));

    mempool_wrapper.add_tx(propagateor_args).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_propagated_tx_sent_to_p2p(mempool: Mempool) {
    // add_tx_input! creates an Invoke Transaction
    let tx_args = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 3, account_nonce: 2);
    let expected_message_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let propagated_args = AddTransactionArgsWrapper {
        args: tx_args.clone(),
        p2p_message_metadata: Some(expected_message_metadata.clone()),
    };

    let mut mock_mempool_p2p_propagator_client = MockMempoolP2pPropagatorClient::new();
    mock_mempool_p2p_propagator_client
        .expect_continue_propagation()
        .times(1)
        .with(predicate::eq(expected_message_metadata.clone()))
        .returning(|_| Ok(()));

    let mut mempool_wrapper =
        MempoolCommunicationWrapper::new(mempool, Arc::new(mock_mempool_p2p_propagator_client));

    mempool_wrapper.add_tx(propagated_args).await.unwrap();
}

#[rstest]
fn test_rejected_tx_deleted_from_mempool(mut mempool: Mempool) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    // Setup. The tip is used here to control the order of transactions in the mempool.
    let tx_address_1_rejected =
        add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 2, account_nonce: 2, tip: 1);
    let tx_address_1_not_executed =
        add_tx_input!(tx_hash: 5, address: "0x1", tx_nonce: 3, account_nonce: 2, tip: 1);

    let tx_address_2_accepted =
        add_tx_input!(tx_hash: 7, address: "0x2", tx_nonce: 1, account_nonce: 1, tip: 0);
    let tx_address_2_rejected =
        add_tx_input!(tx_hash: 8, address: "0x2", tx_nonce: 2, account_nonce: 1, tip: 0);

    let mut expected_pool_txs = vec![];
    for input in [
        &tx_address_1_rejected,
        &tx_address_2_accepted,
        &tx_address_1_not_executed,
        &tx_address_2_rejected,
    ] {
        add_tx(&mut mempool, input);
        expected_pool_txs.push(input.tx.clone());
    }

    // Assert initial mempool content.
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_pool_txs.clone())
        .with_priority_queue(
            [&tx_address_1_rejected.tx, &tx_address_2_accepted.tx].map(TransactionReference::new),
        )
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Test and assert: get all transactions from the Mempool.
    get_txs_and_assert_expected(&mut mempool, expected_pool_txs.len(), &expected_pool_txs);

    // Commit block with transactions 4 and 8 rejected.
    let rejected_tx = [tx_address_1_rejected.tx.tx_hash, tx_address_2_rejected.tx.tx_hash];
    commit_block(&mut mempool, [("0x2", 2)], rejected_tx);

    // Assert transactions 4 and 8 are removed from the mempool.
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool([tx_address_1_not_executed.tx])
        .with_priority_queue(vec![])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Assert: metrics.
    let expected_metrics = MempoolMetrics {
        txs_received_invoke: 4,
        txs_dropped_rejected: 2,
        txs_committed: 1,
        pool_size: 1,
        get_txs_size: 4,
        transaction_time_spent_in_mempool: HistogramValue { count: 3, ..Default::default() },
        ..Default::default()
    };
    expected_metrics.verify_metrics(&recorder);
}

#[rstest]
fn tx_from_address_exists(mut mempool: Mempool) {
    const ACCOUNT_ADDRESS: &str = "0x1";
    let account_address = contract_address!(ACCOUNT_ADDRESS);

    // Account is not known to the mempool.
    assert_eq!(mempool.account_tx_in_pool_or_recent_block(account_address), false);

    // The account has a tx in the mempool.
    add_tx(
        &mut mempool,
        &add_tx_input!(tx_hash: 1, address: ACCOUNT_ADDRESS, tx_nonce: 0, account_nonce: 0),
    );
    assert_eq!(mempool.account_tx_in_pool_or_recent_block(account_address), true);

    // The account has a staged tx in the mempool.
    let get_tx_response = mempool.get_txs(1).unwrap();
    assert_eq!(get_tx_response.first().unwrap().contract_address(), account_address);
    assert_eq!(mempool.account_tx_in_pool_or_recent_block(account_address), true);

    // The account has no txs in the pool, but is known through a committed block.
    commit_block(&mut mempool, [(ACCOUNT_ADDRESS, 1)], []);
    MempoolTestContentBuilder::new().with_pool([]).build().assert_eq(&mempool.content());
    assert_eq!(mempool.account_tx_in_pool_or_recent_block(account_address), true);
}

#[rstest]
fn add_tx_old_transactions_cleanup() {
    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(
        MempoolConfig { transaction_ttl: Duration::from_secs(60), ..Default::default() },
        fake_clock.clone(),
    );

    // Add a new transaction to the mempool.
    let first_tx =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0, tip: 100);
    add_tx(&mut mempool, &first_tx);

    // Advance the clock and add another transaction.
    fake_clock.advance(mempool.config.transaction_ttl / 2);
    let second_tx =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 0, account_nonce: 0, tip: 50);
    add_tx(&mut mempool, &second_tx);

    // Verify that both transactions are in the mempool.
    let expected_txs = [&first_tx.tx, &second_tx.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_txs.map(|tx| tx.clone()))
        .with_priority_queue(expected_txs.map(TransactionReference::new))
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Advance the clock and add a new transaction.
    fake_clock.advance(mempool.config.transaction_ttl / 2 + Duration::from_secs(5));
    let third_tx =
        add_tx_input!(tx_hash: 3, address: "0x2", tx_nonce: 0, account_nonce: 0, tip: 10);
    add_tx(&mut mempool, &third_tx);

    // The first transaction should be removed from the mempool.
    let expected_txs = [&second_tx.tx, &third_tx.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_txs.map(|tx| tx.clone()))
        .with_priority_queue(expected_txs.map(TransactionReference::new))
        .build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn get_txs_old_transactions_cleanup() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(
        MempoolConfig { transaction_ttl: Duration::from_secs(60), ..Default::default() },
        fake_clock.clone(),
    );

    // Add a new transaction to the mempool.
    let first_tx =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0, tip: 100);
    add_tx(&mut mempool, &first_tx);

    // Advance the clock and add another transaction.
    fake_clock.advance(mempool.config.transaction_ttl / 2);

    let second_tx =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 0, account_nonce: 0, tip: 50);
    add_tx(&mut mempool, &second_tx);

    // Verify that both transactions are in the mempool.
    let expected_txs = [&first_tx.tx, &second_tx.tx];
    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool(expected_txs.map(|tx| tx.clone()))
        .with_priority_queue(expected_txs.map(TransactionReference::new))
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Advance the clock. Now only the second transaction should be returned from get_txs, and the
    // first should be removed.
    fake_clock.advance(mempool.config.transaction_ttl / 2 + Duration::from_secs(5));

    assert_eq!(mempool.get_txs(2).unwrap(), vec![second_tx.tx.clone()]);

    let expected_mempool_content = MempoolTestContentBuilder::new()
        .with_pool([second_tx.tx.clone()])
        .with_priority_queue([])
        .with_pending_queue([])
        .build();
    expected_mempool_content.assert_eq(&mempool.content());

    // Assert: metrics.
    let expected_metrics = MempoolMetrics {
        txs_received_invoke: 2,
        txs_dropped_expired: 1,
        pool_size: 1,
        get_txs_size: 1,
        transaction_time_spent_in_mempool: HistogramValue {
            sum: 65.0,
            count: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    expected_metrics.verify_metrics(&recorder);
}

#[test]
fn test_register_metrics() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let expected_metrics = MempoolMetrics::default();
    expected_metrics.verify_metrics(&recorder);
}

#[rstest]
fn expired_staged_txs_are_not_deleted() {
    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(
        MempoolConfig { transaction_ttl: Duration::from_secs(60), ..Default::default() },
        fake_clock.clone(),
    );

    // Add 2 transactions to the mempool, and stage one.
    let staged_tx =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0, tip: 100);
    let nonstaged_tx =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, account_nonce: 0, tip: 100);
    add_tx(&mut mempool, &staged_tx);
    add_tx(&mut mempool, &nonstaged_tx);
    assert_eq!(mempool.get_txs(1).unwrap(), vec![staged_tx.tx.clone()]);

    // Advance the clock beyond the TTL.
    fake_clock.advance(mempool.config.transaction_ttl + Duration::from_secs(5));

    // Add another transaction to trigger the cleanup, and verify the staged tx is still in the
    // mempool. The non-staged tx should be removed.
    let another_tx =
        add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 0, account_nonce: 0, tip: 100);
    add_tx(&mut mempool, &another_tx);
    let expected_mempool_content =
        MempoolTestContentBuilder::new().with_pool([staged_tx.tx, another_tx.tx]).build();
    expected_mempool_content.assert_eq(&mempool.content());
}

#[rstest]
fn delay_declare_txs() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let declare_delay = Duration::from_secs(5);
    let mut mempool =
        Mempool::new(MempoolConfig { declare_delay, ..Default::default() }, fake_clock.clone());
    let first_declare = declare_add_tx_input(
        declare_tx_args!(resource_bounds: test_valid_resource_bounds(), sender_address: contract_address!("0x0"), tx_hash: tx_hash!(0)),
    );
    add_tx(&mut mempool, &first_declare);

    fake_clock.advance(Duration::from_secs(1));
    let second_declare = declare_add_tx_input(
        declare_tx_args!(resource_bounds: test_valid_resource_bounds(), sender_address: contract_address!("0x1"), tx_hash: tx_hash!(1)),
    );
    add_tx(&mut mempool, &second_declare);

    assert_eq!(mempool.get_txs(2).unwrap(), vec![]);

    // Assert: metrics.
    let expected_metrics =
        MempoolMetrics { txs_received_declare: 2, delayed_declares_size: 2, ..Default::default() };
    expected_metrics.verify_metrics(&recorder);

    // Complete the first declare's delay.
    fake_clock.advance(declare_delay - Duration::from_secs(1));
    // Add another transaction to trigger `add_ready_declares`.
    let another_tx_1 =
        add_tx_input!(tx_hash: 123, address: "0x123", tx_nonce: 123, account_nonce: 0, tip: 123);
    add_tx(&mut mempool, &another_tx_1);

    // Assert only the first declare is in the mempool.
    assert_eq!(mempool.get_txs(2).unwrap(), vec![first_declare.tx]);

    // Complete the second declare's delay.
    fake_clock.advance(Duration::from_secs(1));
    // Add another transaction to trigger `add_ready_declares`
    let another_tx_2 =
        add_tx_input!(tx_hash: 2, address: "0x1", tx_nonce: 5, account_nonce: 0, tip: 100);
    add_tx(&mut mempool, &another_tx_2);

    // Assert the second declare was also added to the mempool.
    assert_eq!(mempool.get_txs(2).unwrap(), vec![second_declare.tx]);
}

#[rstest]
fn no_delay_declare_front_run() {
    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(
        MempoolConfig {
            declare_delay: Duration::from_secs(5),
            // Always accept fee escalation to test only the delayed declare duplicate nonce.
            enable_fee_escalation: true,
            fee_escalation_percentage: 0,
            ..Default::default()
        },
        fake_clock.clone(),
    );
    let declare = declare_add_tx_input(
        declare_tx_args!(resource_bounds: test_valid_resource_bounds(), sender_address: contract_address!("0x0"), tx_hash: tx_hash!(0)),
    );
    add_tx(&mut mempool, &declare);
    add_tx_expect_error(
        &mut mempool,
        &declare,
        MempoolError::DuplicateNonce {
            address: declare.tx.contract_address(),
            nonce: declare.tx.nonce(),
        },
    );
}

#[rstest]
fn committed_account_nonce_cleanup() {
    let mut mempool = Mempool::new(
        MempoolConfig { committed_nonce_retention_block_count: 2, ..Default::default() },
        Arc::new(FakeClock::default()),
    );

    // Setup: commit a block with account nonce 1.
    commit_block(&mut mempool, [("0x0", 1)], []);

    // Add a transaction with nonce 0. Should be rejected with NonceTooOld.
    let input_tx = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    add_tx_expect_error(
        &mut mempool,
        &input_tx,
        MempoolError::NonceTooOld { address: contract_address!("0x0"), nonce: nonce!(0) },
    );

    // Commit an empty block, and check the transaction is still rejected.
    commit_block(&mut mempool, [], []);
    add_tx_expect_error(
        &mut mempool,
        &input_tx,
        MempoolError::NonceTooOld { address: contract_address!("0x0"), nonce: nonce!(0) },
    );

    // Commit another empty block. This should remove the previously committed nonce, and
    // the transaction should be accepted.
    commit_block(&mut mempool, [], []);
    add_tx(&mut mempool, &input_tx);
}

#[rstest]
fn test_get_mempool_snapshot() {
    // Create a mempool with a fake clock.
    let fake_clock = Arc::new(FakeClock::default());
    let mut mempool = Mempool::new(MempoolConfig::default(), fake_clock.clone());

    for i in 1..10 {
        fake_clock.advance(Duration::from_secs(1));
        add_tx(
            &mut mempool,
            &add_tx_input!(tx_hash: i, address: format!("0x{}", i).as_str(), tip: 10),
        );
    }

    // Test.
    let mempool_snapshot = mempool.get_mempool_snapshot().unwrap();

    // Check that the returned hashes are sorted by submission time.
    let expected_chronological_hashes = (1..10).rev().map(|i| tx_hash!(i)).collect::<Vec<_>>();

    assert_eq!(mempool_snapshot.transactions, expected_chronological_hashes);
}
