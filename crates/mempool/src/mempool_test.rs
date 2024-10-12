use std::cmp::Reverse;

use mempool_test_utils::starknet_api_test_utils::VALID_L2_GAS_MAX_PRICE_PER_UNIT;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::{
    AllResourceBounds,
    ResourceBounds,
    Tip,
    TransactionHash,
    ValidResourceBounds,
};
use starknet_api::{contract_address, felt, invoke_tx_args, nonce, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};

use crate::mempool::{Mempool, TransactionReference};
use crate::test_utils::{add_tx, add_tx_expect_error, commit_block, get_txs_and_assert_expected};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::transaction_queue_test_utils::{
    TransactionQueueContent,
    TransactionQueueContentBuilder,
};
use crate::transaction_queue::TransactionQueue;
use crate::{add_tx_input, tx};

// Utils.

/// Represents the internal content of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct MempoolContent {
    tx_pool: Option<TransactionPool>,
    tx_queue_content: Option<TransactionQueueContent>,
    fee_escalation_percentage: u8,
}

impl MempoolContent {
    fn assert_eq(&self, mempool: &Mempool) {
        if let Some(tx_pool) = &self.tx_pool {
            assert_eq!(&mempool.tx_pool, tx_pool);
        }

        if let Some(tx_queue_content) = &self.tx_queue_content {
            tx_queue_content.assert_eq(&mempool.tx_queue);
        }
    }
}

impl From<MempoolContent> for Mempool {
    fn from(mempool_content: MempoolContent) -> Mempool {
        let MempoolContent { tx_pool, tx_queue_content, fee_escalation_percentage } =
            mempool_content;
        Mempool {
            tx_pool: tx_pool.unwrap_or_default(),
            tx_queue: tx_queue_content
                .map(|content| content.complete_to_tx_queue())
                .unwrap_or_default(),
            // TODO: Add implementation when needed.
            mempool_state: Default::default(),
            account_nonces: Default::default(),
            fee_escalation_percentage,
        }
    }
}

#[derive(Debug, Default)]
struct MempoolContentBuilder {
    tx_pool: Option<TransactionPool>,
    tx_queue_content_builder: TransactionQueueContentBuilder,
    fee_escalation_percentage: u8,
}

impl MempoolContentBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn with_pool<P>(mut self, pool_txs: P) -> Self
    where
        P: IntoIterator<Item = Transaction>,
    {
        self.tx_pool = Some(pool_txs.into_iter().collect());
        self
    }

    fn with_priority_queue<Q>(mut self, queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        self.tx_queue_content_builder = self.tx_queue_content_builder.with_priority(queue_txs);
        self
    }

    fn _with_pending_queue<Q>(mut self, queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        self.tx_queue_content_builder = self.tx_queue_content_builder._with_pending(queue_txs);
        self
    }

    fn with_fee_escalation_percentage(mut self, fee_escalation_percentage: u8) -> Self {
        self.fee_escalation_percentage = fee_escalation_percentage;
        self
    }

    fn build(self) -> MempoolContent {
        MempoolContent {
            tx_pool: self.tx_pool,
            tx_queue_content: self.tx_queue_content_builder.build(),
            fee_escalation_percentage: self.fee_escalation_percentage,
        }
    }

    fn build_into_mempool(self) -> Mempool {
        self.build().into()
    }
}

impl FromIterator<Transaction> for TransactionPool {
    fn from_iter<T: IntoIterator<Item = Transaction>>(txs: T) -> Self {
        let mut pool = Self::default();
        for tx in txs {
            pool.insert(tx).unwrap();
        }
        pool
    }
}

impl FromIterator<TransactionReference> for TransactionQueue {
    fn from_iter<T: IntoIterator<Item = TransactionReference>>(txs: T) -> Self {
        let mut queue = Self::default();
        for tx in txs {
            queue.insert(tx);
        }
        queue
    }
}

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// Tests.

// `get_txs` tests.

#[rstest]
#[case::test_get_zero_txs(0)]
#[case::test_get_exactly_all_eligible_txs(3)]
#[case::test_get_more_than_all_eligible_txs(5)]
#[case::test_get_less_than_all_eligible_txs(2)]
fn test_get_txs_returns_by_priority_order(#[case] n_requested_txs: usize) {
    // Setup.
    let mut txs = [
        tx!(tip: 20, tx_hash: 1, sender_address: "0x0"),
        tx!(tip: 30, tx_hash: 2, sender_address: "0x1"),
        tx!(tip: 10, tx_hash: 3, sender_address: "0x2"),
    ];

    let mut mempool = MempoolContentBuilder::new()
        .with_pool(txs.iter().cloned())
        .with_priority_queue(txs.iter().map(TransactionReference::new))
        .build_into_mempool();

    // Test.
    let fetched_txs = mempool.get_txs(n_requested_txs).unwrap();

    // Check that the returned transactions are the ones with the highest priority.
    txs.sort_by_key(|tx| Reverse(tx.tip()));
    let (expected_fetched_txs, remaining_txs) = txs.split_at(fetched_txs.len());
    assert_eq!(fetched_txs, expected_fetched_txs);

    // Assert: non-returned transactions are still in the mempool.
    let remaining_tx_references = remaining_txs.iter().map(TransactionReference::new);
    let mempool_content =
        MempoolContentBuilder::new().with_priority_queue(remaining_tx_references).build();
    mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_get_txs_does_not_remove_returned_txs_from_pool() {
    // Setup.
    let tx_nonce_0 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0);
    let tx_nonce_1 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1);
    let tx_nonce_2 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 2);

    let queue_txs = [TransactionReference::new(&tx_nonce_0)];
    let pool_txs = [tx_nonce_0, tx_nonce_1, tx_nonce_2];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test and assert: all transactions are returned.
    get_txs_and_assert_expected(&mut mempool, 3, &pool_txs);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_only_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 = tx!(tip: 20, tx_hash: 1, sender_address: "0x0", tx_nonce: 0);
    let tx_address_0_nonce_1 = tx!(tip: 20, tx_hash: 2, sender_address: "0x0", tx_nonce: 1);
    let tx_address_1_nonce_0 = tx!(tip: 10, tx_hash: 3, sender_address: "0x1", tx_nonce: 0);

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs =
        [&tx_address_0_nonce_0, &tx_address_0_nonce_1, &tx_address_1_nonce_0].map(|tx| tx.clone());
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test and assert: all transactions returned.
    // Replenishment done in chunks: account 1 transaction is returned before the one of account 0,
    // although its priority is higher.
    get_txs_and_assert_expected(
        &mut mempool,
        3,
        &[tx_address_0_nonce_0, tx_address_1_nonce_0, tx_address_0_nonce_1],
    );
    let expected_mempool_content = MempoolContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_multi_account_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 = tx!(tip: 30, tx_hash: 1, sender_address: "0x0", tx_nonce: 0);
    let tx_address_0_nonce_1 = tx!(tip: 30, tx_hash: 3, sender_address: "0x0", tx_nonce: 1);
    let tx_address_1_nonce_0 = tx!(tip: 20, tx_hash: 2, sender_address: "0x1", tx_nonce: 0);
    let tx_address_1_nonce_1 = tx!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 1);

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs = [
        &tx_address_0_nonce_0,
        &tx_address_1_nonce_0,
        &tx_address_0_nonce_1,
        &tx_address_1_nonce_1,
    ]
    .map(|tx| tx.clone());
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test and assert: queue is replenished with the next transactions of each account.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_address_0_nonce_0, tx_address_1_nonce_0]);
    let expected_queue_txs =
        [&tx_address_0_nonce_1, &tx_address_1_nonce_1].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_get_txs_with_nonce_gap() {
    // Setup.
    let tx_address_0_nonce_1 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1);
    let tx_address_1_nonce_0 = tx!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0);

    let queue_txs = [TransactionReference::new(&tx_address_1_nonce_0)];
    let pool_txs = [tx_address_0_nonce_1, tx_address_1_nonce_0.clone()];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test and assert.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_address_1_nonce_0]);
    let expected_mempool_content = MempoolContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool);
}

// TODO(Mohammad): simplify two queues reordering tests to use partial queue content test util.
#[rstest]
fn test_get_txs_while_decreasing_gas_price_threshold() {
    // Setup.
    let tx = tx!(tx_nonce: 0);

    let mut mempool = MempoolContentBuilder::new()
        .with_pool([tx.clone()])
        .with_priority_queue([TransactionReference::new(&tx)])
        .build_into_mempool();

    // Test.
    // High gas price threshold, no transactions should be returned.
    mempool._update_gas_price_threshold(GasPrice(1000000000000));
    get_txs_and_assert_expected(&mut mempool, 1, &[]);

    // Low gas price threshold, the transaction should be returned.
    mempool._update_gas_price_threshold(GasPrice(100));
    get_txs_and_assert_expected(&mut mempool, 1, &[tx]);
}

#[rstest]
fn test_get_txs_while_increasing_gas_price_threshold() {
    // Setup.
    // Both transactions have the same gas price.
    let tx_nonce_0 = tx!(tx_hash: 0, tx_nonce: 0);
    let tx_nonce_1 = tx!(tx_hash: 1, tx_nonce: 1);

    let mut mempool = MempoolContentBuilder::new()
        .with_pool([tx_nonce_0.clone(), tx_nonce_1])
        .with_priority_queue([TransactionReference::new(&tx_nonce_0)])
        .build_into_mempool();

    // Test.
    // Low gas price threshold, the transaction should be returned.
    mempool._update_gas_price_threshold(GasPrice(100));
    get_txs_and_assert_expected(&mut mempool, 1, &[tx_nonce_0]);

    // High gas price threshold, no transactions should be returned.
    mempool._update_gas_price_threshold(GasPrice(1000000000000));
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

// `add_tx` tests.

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    // Setup.
    let mut add_tx_inputs = [
        add_tx_input!(tip: 50, tx_hash: 1, sender_address: "0x0", tx_nonce: 0, account_nonce: 0),
        add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1", tx_nonce: 1, account_nonce: 1),
        add_tx_input!(tip: 80, tx_hash: 3, sender_address: "0x2", tx_nonce: 2, account_nonce: 2),
    ];

    // Test.
    for input in &add_tx_inputs {
        add_tx(&mut mempool, input);
    }

    // TODO(Ayelet): Consider share this code.
    // Sort in an ascending priority order.
    add_tx_inputs.sort_by_key(|input| std::cmp::Reverse(input.tx.tip().unwrap()));

    // Assert: transactions are ordered by priority.
    let expected_queue_txs: Vec<TransactionReference> =
        add_tx_inputs.iter().map(|input| TransactionReference::new(&input.tx)).collect();
    let expected_pool_txs = add_tx_inputs.into_iter().map(|input| input.tx);
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_multi_nonce_success(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0, account_nonce: 0);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 1, account_nonce: 0);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 2, sender_address: "0x1", tx_nonce: 0,account_nonce: 0);

    // Test.
    for input in [&input_address_0_nonce_0, &input_address_1_nonce_0, &input_address_0_nonce_1] {
        add_tx(&mut mempool, input);
    }

    // Assert: only the eligible transactions appear in the queue.
    let expected_queue_txs =
        [&input_address_1_nonce_0.tx, &input_address_0_nonce_0.tx].map(TransactionReference::new);
    let expected_pool_txs =
        [input_address_0_nonce_0.tx, input_address_1_nonce_0.tx, input_address_0_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_failure_on_duplicate_tx_hash(mut mempool: Mempool) {
    // Setup.
    let input = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);
    let duplicate_input = input.clone(); // Same hash is possible if signature is different.

    // Test.
    add_tx(&mut mempool, &input);
    add_tx_expect_error(
        &mut mempool,
        &duplicate_input,
        MempoolError::DuplicateTransaction { tx_hash: input.tx.tx_hash() },
    );

    // Assert: the original transaction remains.
    let expected_mempool_content = MempoolContentBuilder::new().with_pool([input.tx]).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_lower_than_queued_nonce() {
    // Setup.
    let tx = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 1);
    let queue_txs = [TransactionReference::new(&tx)];
    let pool_txs = [tx];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test and assert: original transaction remains.
    for tx_nonce in [0, 1] {
        let invalid_input =
            add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: tx_nonce, account_nonce: 0);
        add_tx_expect_error(
            &mut mempool,
            &invalid_input,
            MempoolError::DuplicateNonce {
                address: contract_address!("0x0"),
                nonce: nonce!(tx_nonce),
            },
        );
    }

    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_priority_queue(queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_updates_queue_with_higher_account_nonce() {
    // Setup.
    let input = add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0, account_nonce: 0);
    let higher_account_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1, account_nonce: 1);

    let queue_txs = [TransactionReference::new(&input.tx)];
    let mut mempool =
        MempoolContentBuilder::new().with_priority_queue(queue_txs).build_into_mempool();

    // Test.
    add_tx(&mut mempool, &higher_account_nonce_input);

    // Assert: the higher account nonce transaction is in the queue.
    let expected_queue_txs = [TransactionReference::new(&higher_account_nonce_input.tx)];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_with_identical_tip_succeeds(mut mempool: Mempool) {
    // Setup.
    let input1 = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x0");
    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tip: 1, tx_hash: 1, sender_address: "0x1");

    // Test.
    for input in [&input1, &input2] {
        add_tx(&mut mempool, input);
    }

    // Assert: both transactions are in the mempool.
    let expected_queue_txs = [&input1.tx, &input2.tx].map(TransactionReference::new);
    let expected_pool_txs = [input1.tx, input2.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_delete_tx_with_lower_nonce_than_account_nonce() {
    // Setup.
    let tx_nonce_0_account_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0, account_nonce: 0);
    let tx_nonce_1_account_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1, account_nonce: 1);

    let queue_txs = [TransactionReference::new(&tx_nonce_0_account_nonce_0.tx)];
    let pool_txs = [tx_nonce_0_account_nonce_0.tx];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    add_tx(&mut mempool, &tx_nonce_1_account_nonce_1);

    // Assert the transaction with the lower nonce is removed.
    let expected_queue_txs = [TransactionReference::new(&tx_nonce_1_account_nonce_1.tx)];
    let expected_pool_txs = [tx_nonce_1_account_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_tip_priority_over_tx_hash(mut mempool: Mempool) {
    // Setup.
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: 1, sender_address: "0x0");
    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x1");

    // Test.
    for input in [&input_big_tip_small_hash, &input_small_tip_big_hash] {
        add_tx(&mut mempool, input);
    }

    // Assert: ensure that the transaction with the higher tip is prioritized higher.
    let expected_queue_txs =
        [&input_big_tip_small_hash.tx, &input_small_tip_big_hash.tx].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_account_state_fills_nonce_gap(mut mempool: Mempool) {
    // Setup.
    let tx_input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);
    // Input that increments the account state.
    let tx_input_nonce_2 = add_tx_input!(tx_hash: 2, tx_nonce: 2, account_nonce: 1);

    // Test and assert.

    // First, with gap.
    add_tx(&mut mempool, &tx_input_nonce_1);
    let expected_mempool_content = MempoolContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool);

    // Then, fill it.
    add_tx(&mut mempool, &tx_input_nonce_2);
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_priority_queue([TransactionReference::new(&tx_input_nonce_1.tx)])
        .build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_add_tx_sequential_nonces(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 0, tx_nonce: 0, account_nonce: 0);
    let input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);

    // Test.
    for input in [&input_nonce_0, &input_nonce_1] {
        add_tx(&mut mempool, input);
    }

    // Assert: only eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_0.tx, input_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
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
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(expected_pool_txs).with_priority_queue([]).build();
    expected_mempool_content.assert_eq(&mempool);

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_fee_escalation() {
    let mut mempool =
        MempoolContentBuilder::new().with_fee_escalation_percentage(10).build_into_mempool();

    // Valid replacement.

    // Setup.
    let input = add_tx_input!(tx_hash: 1, tip: 90, max_l2_gas_price: 90, tx_nonce: 1, sender_address: "0x0");
    let valid_replacement_input = add_tx_input!(tx_hash: 2, tip: 100, max_l2_gas_price: 100, tx_nonce: 1, sender_address: "0x0");

    // Test and assert.
    add_tx(&mut mempool, &input);
    add_tx(&mut mempool, &valid_replacement_input);

    // Verify transaction was replaced.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool([valid_replacement_input.tx.clone()]).build();
    expected_mempool_content.assert_eq(&mempool);

    // Invalid replacement.

    // Setup.
    let input_not_enough_tip = add_tx_input!(tx_hash: 3, tip: 109, max_l2_gas_price: 110, tx_nonce: 1, sender_address: "0x0");
    let input_not_enough_gas_price = add_tx_input!(tx_hash: 4, tip: 110, max_l2_gas_price: 109, tx_nonce: 1, sender_address: "0x0");
    let input_not_enough_both = add_tx_input!(tx_hash: 5, tip: 109, max_l2_gas_price: 109, tx_nonce: 1, sender_address: "0x0");

    // Test and assert.
    for input in [&input_not_enough_tip, &input_not_enough_gas_price, &input_not_enough_both] {
        add_tx_expect_error(
            &mut mempool,
            input,
            MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(1) },
        );
    }

    // Assert: transaction was not replaced.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool([valid_replacement_input.tx]).build();
    expected_mempool_content.assert_eq(&mempool);
}

// `commit_block` tests.

#[rstest]
fn test_commit_block_includes_all_proposed_txs() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_address_0_nonce_4 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 4);
    let tx_address_0_nonce_5 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 5);
    let tx_address_1_nonce_2 = tx!(tx_hash: 4, sender_address: "0x1", tx_nonce: 2);
    let tx_address_1_nonce_3 = tx!(tx_hash: 5, sender_address: "0x1", tx_nonce: 3);
    let tx_address_2_nonce_1 = tx!(tx_hash: 6, sender_address: "0x2", tx_nonce: 1);

    let queue_txs = [&tx_address_0_nonce_4, &tx_address_1_nonce_3, &tx_address_2_nonce_1]
        .map(TransactionReference::new);
    let pool_txs = [
        tx_address_0_nonce_3,
        tx_address_0_nonce_4.clone(),
        tx_address_0_nonce_5.clone(),
        tx_address_1_nonce_2,
        tx_address_1_nonce_3.clone(),
        tx_address_2_nonce_1.clone(),
    ];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    let nonces = [("0x0", 3), ("0x1", 2)];
    let tx_hashes = [1, 4];
    commit_block(&mut mempool, nonces, tx_hashes);

    // Assert.
    let pool_txs =
        [tx_address_0_nonce_4, tx_address_0_nonce_5, tx_address_1_nonce_3, tx_address_2_nonce_1];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_priority_queue(queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_commit_block_rewinds_queued_nonce() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_address_0_nonce_4 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 4);
    let tx_address_0_nonce_5 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 5);
    let tx_address_1_nonce_1 = tx!(tx_hash: 4, sender_address: "0x1", tx_nonce: 1);

    let queued_txs = [&tx_address_0_nonce_5, &tx_address_1_nonce_1].map(TransactionReference::new);
    let pool_txs = [
        tx_address_0_nonce_3,
        tx_address_0_nonce_4.clone(),
        tx_address_0_nonce_5,
        tx_address_1_nonce_1,
    ];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let nonces = [("0x0", 3), ("0x1", 1)];
    let tx_hashes = [1, 4];
    commit_block(&mut mempool, nonces, tx_hashes);

    // Assert.
    let expected_queue_txs = [TransactionReference::new(&tx_address_0_nonce_4)];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq(&mempool);
}

#[rstest]
fn test_commit_block_from_different_leader() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5);
    let tx_address_0_nonce_6 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 6);
    let tx_address_1_nonce_2 = tx!(tx_hash: 4, sender_address: "0x1", tx_nonce: 2);

    let queued_txs = [TransactionReference::new(&tx_address_1_nonce_2)];
    let pool_txs = [
        tx_address_0_nonce_3,
        tx_address_0_nonce_5,
        tx_address_0_nonce_6.clone(),
        tx_address_1_nonce_2.clone(),
    ];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let nonces = [
        ("0x0", 5),
        ("0x1", 0), // A hole, missing nonce 1 for address "0x1".
        ("0x2", 1),
    ];
    let tx_hashes = [
        1, 2, // Hashes known to mempool.
        5, 6, // Hashes unknown to mempool, from a different node.
    ];
    commit_block(&mut mempool, nonces, tx_hashes);

    // Assert.
    let expected_queue_txs = [TransactionReference::new(&tx_address_0_nonce_6)];
    let expected_pool_txs = [tx_address_0_nonce_6, tx_address_1_nonce_2];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq(&mempool);
}
