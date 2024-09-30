use std::cmp::Reverse;
use std::collections::HashMap;

use assert_matches::assert_matches;
use mempool_test_utils::starknet_api_test_utils::test_resource_bounds_mapping;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_api::{contract_address, felt, invoke_tx_args, nonce, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::AccountState;

use crate::mempool::{AccountToNonce, Mempool, MempoolInput, TransactionReference};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::transaction_queue_test_utils::{
    TransactionQueueContent,
    TransactionQueueContentBuilder,
};
use crate::transaction_queue::TransactionQueue;

// Utils.

/// Represents the internal content of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct MempoolContent {
    tx_pool: Option<TransactionPool>,
    tx_queue_content: Option<TransactionQueueContent>,
    account_nonces: Option<AccountToNonce>,
}

impl MempoolContent {
    fn assert_eq_pool_and_priority_queue_content(&self, mempool: &Mempool) {
        self.assert_eq_tx_pool_content(mempool);
        self.assert_eq_tx_priority_queue_content(mempool);
    }

    fn assert_eq_tx_pool_content(&self, mempool: &Mempool) {
        assert_eq!(self.tx_pool.as_ref().unwrap(), &mempool.tx_pool);
    }

    fn assert_eq_tx_priority_queue_content(&self, mempool: &Mempool) {
        self.tx_queue_content.as_ref().unwrap().assert_eq_priority_queue_content(&mempool.tx_queue);
    }

    fn _assert_eq_tx_pending_queue_content(&self, mempool: &Mempool) {
        self.tx_queue_content.as_ref().unwrap()._assert_eq_pending_queue_content(&mempool.tx_queue);
    }

    fn assert_eq_account_nonces(&self, mempool: &Mempool) {
        assert_eq!(self.account_nonces.as_ref().unwrap(), &mempool.account_nonces);
    }
}

impl From<MempoolContent> for Mempool {
    fn from(mempool_content: MempoolContent) -> Mempool {
        let MempoolContent { tx_pool, tx_queue_content, account_nonces } = mempool_content;
        Mempool {
            tx_pool: tx_pool.unwrap_or_default(),
            tx_queue: tx_queue_content
                .map(|content| content.complete_to_tx_queue())
                .unwrap_or_default(),
            // TODO: Add implementation when needed.
            mempool_state: Default::default(),
            account_nonces: account_nonces.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
struct MempoolContentBuilder {
    tx_pool: Option<TransactionPool>,
    tx_queue_content_builder: TransactionQueueContentBuilder,
    account_nonces: Option<AccountToNonce>,
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

    fn with_account_nonces<A>(mut self, account_nonce_pairs: A) -> Self
    where
        A: IntoIterator<Item = (&'static str, u8)>,
    {
        self.account_nonces = Some(
            account_nonce_pairs
                .into_iter()
                .map(|(address, nonce)| (contract_address!(address), nonce!(nonce)))
                .collect(),
        );
        self
    }

    fn build(self) -> MempoolContent {
        MempoolContent {
            tx_pool: self.tx_pool,
            tx_queue_content: self.tx_queue_content_builder.build(),
            account_nonces: self.account_nonces,
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

#[track_caller]
fn add_tx(mempool: &mut Mempool, input: &MempoolInput) {
    assert_eq!(mempool.add_tx(input.clone()), Ok(()));
}

#[track_caller]
fn add_tx_expect_error(mempool: &mut Mempool, input: &MempoolInput, expected_error: MempoolError) {
    assert_eq!(mempool.add_tx(input.clone()), Err(expected_error));
}

#[track_caller]
fn commit_block(
    mempool: &mut Mempool,
    state_changes: impl IntoIterator<Item = (&'static str, u8)>,
) {
    let state_changes = HashMap::from_iter(
        state_changes
            .into_iter()
            .map(|(address, nonce)| (contract_address!(address), nonce!(nonce))),
    );
    assert_eq!(mempool.commit_block(state_changes), Ok(()));
}

#[track_caller]
fn get_txs_and_assert_expected(mempool: &mut Mempool, n_txs: usize, expected_txs: &[Transaction]) {
    let txs = mempool.get_txs(n_txs).unwrap();
    assert_eq!(txs, expected_txs);
}

/// Creates an executable invoke transaction with the given field subset (the rest receive default
/// values).
macro_rules! tx {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, resource_bounds: $resource_bounds:expr) => {{
            let sender_address = contract_address!($sender_address);
            Transaction::Invoke(executable_invoke_tx(invoke_tx_args!{
                sender_address: sender_address,
                tx_hash: TransactionHash(StarkHash::from($tx_hash)),
                tip: Tip($tip),
                nonce: nonce!($tx_nonce),
                resource_bounds: $resource_bounds,
            }))
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, resource_bounds: ValidResourceBounds::AllResources(test_resource_bounds_mapping()))
    };
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce)
    };
    (tx_nonce: $tx_nonce:expr) => {
        tx!(tip: 0, tx_hash: 0, sender_address: "0x0", tx_nonce: $tx_nonce)
    };
}

/// Creates an input for `add_tx` with the given field subset (the rest receive default values).
macro_rules! add_tx_input {
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr, resource_bounds: $resource_bounds:expr) => {{
        let tx = tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, resource_bounds: $resource_bounds);
        let address = contract_address!($sender_address);
        let account_nonce = nonce!($account_nonce);
        let account_state = AccountState { address, nonce: account_nonce};

        MempoolInput { tx, account_state }
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {{
            add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce, resource_bounds: ValidResourceBounds::AllResources(test_resource_bounds_mapping()))
    }};
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0, account_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: 0, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: 0, account_nonce: 0)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: 0)
    };
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
    mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_removes_returned_txs_from_pool() {
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
        MempoolContentBuilder::new().with_pool([]).with_priority_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
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
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
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
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_with_holes() {
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
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
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
    mempool._update_gas_price_threshold(1000000000000);
    get_txs_and_assert_expected(&mut mempool, 1, &[]);

    // Updating the gas price threshold should happen in a new block creation.
    let state_changes = [];
    commit_block(&mut mempool, state_changes);

    // Low gas price threshold, the transaction should be returned.
    mempool._update_gas_price_threshold(100);
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
    mempool._update_gas_price_threshold(100);
    get_txs_and_assert_expected(&mut mempool, 1, &[tx_nonce_0]);

    // Updating the gas price threshold should happen in a new block creation.
    let state_changes = [];
    commit_block(&mut mempool, state_changes);

    // High gas price threshold, no transactions should be returned.
    mempool._update_gas_price_threshold(1000000000000);
    get_txs_and_assert_expected(&mut mempool, 1, &[]);
}

// `add_tx` tests.

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    // Setup.
    let mut add_tx_inputs = [
        add_tx_input!(tip: 50, tx_hash: 1, sender_address: "0x0"),
        add_tx_input!(tip: 100, tx_hash: 2, sender_address: "0x1"),
        add_tx_input!(tip: 80, tx_hash: 3, sender_address: "0x2"),
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
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
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
    add_tx(&mut mempool, &input_address_0_nonce_0);
    add_tx(&mut mempool, &input_address_1_nonce_0);
    add_tx(&mut mempool, &input_address_0_nonce_1);

    // Assert: only the eligible transactions appear in the queue.
    let expected_queue_txs =
        [&input_address_1_nonce_0.tx, &input_address_0_nonce_0.tx].map(TransactionReference::new);
    let expected_pool_txs =
        [input_address_0_nonce_0.tx, input_address_1_nonce_0.tx, input_address_0_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_with_duplicate_tx(mut mempool: Mempool) {
    // Setup.
    let input = add_tx_input!(tip: 50, tx_hash: 1);
    let duplicate_input = input.clone();

    // Test.
    add_tx(&mut mempool, &input);
    assert_matches!(
        mempool.add_tx(duplicate_input),
        Err(MempoolError::DuplicateTransaction { .. })
    );

    // Assert: the original transaction remains.
    let expected_mempool_content = MempoolContentBuilder::new().with_pool([input.tx]).build();
    expected_mempool_content.assert_eq_tx_pool_content(&mempool);
}

#[rstest]
fn test_add_tx_lower_than_queued_nonce() {
    // Setup.
    let valid_input =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 1, account_nonce: 1);
    let lower_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 0, account_nonce: 0);

    let MempoolInput { tx: valid_input_tx, .. } = valid_input;
    let queue_txs = [TransactionReference::new(&valid_input_tx)];
    let pool_txs = [valid_input_tx];
    let account_nonces = [("0x0", 1)];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .with_account_nonces(account_nonces)
        .build();

    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .with_account_nonces(account_nonces)
        .build_into_mempool();

    // Test and assert the original transaction remains.
    add_tx_expect_error(
        &mut mempool,
        &lower_nonce_input,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(0) },
    );
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
    expected_mempool_content.assert_eq_account_nonces(&mempool);
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
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_with_identical_tip_succeeds(mut mempool: Mempool) {
    // Setup.
    let input1 = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x0");
    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input2 = add_tx_input!(tip: 1, tx_hash: 1, sender_address: "0x1");

    // Test.
    add_tx(&mut mempool, &input1);
    add_tx(&mut mempool, &input2);

    // Assert: both transactions are in the mempool.
    let expected_queue_txs =
        [TransactionReference::new(&input1.tx), TransactionReference::new(&input2.tx)];
    let expected_pool_txs = [input1.tx, input2.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
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
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_tip_priority_over_tx_hash(mut mempool: Mempool) {
    // Setup.
    let input_big_tip_small_hash = add_tx_input!(tip: 2, tx_hash: 1, sender_address: "0x0");
    // Create a transaction with identical tip, it should be allowed through since the priority
    // queue tie-breaks identical tips by other tx-unique identifiers (for example tx hash).
    let input_small_tip_big_hash = add_tx_input!(tip: 1, tx_hash: 2, sender_address: "0x1");

    // Test.
    add_tx(&mut mempool, &input_big_tip_small_hash);
    add_tx(&mut mempool, &input_small_tip_big_hash);

    // Assert: ensure that the transaction with the higher tip is prioritized higher.
    let expected_queue_txs =
        [&input_big_tip_small_hash.tx, &input_small_tip_big_hash.tx].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_account_state_fills_hole(mut mempool: Mempool) {
    // Setup.
    let tx_input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);
    // Input that increments the account state.
    let tx_input_nonce_2 = add_tx_input!(tx_hash: 2, tx_nonce: 2, account_nonce: 1);

    // Test and assert.

    // First, with gap.
    add_tx(&mut mempool, &tx_input_nonce_1);
    let expected_mempool_content = MempoolContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);

    // Then, fill it.
    add_tx(&mut mempool, &tx_input_nonce_2);
    let expected_queue_txs = [&tx_input_nonce_1.tx].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_sequential_nonces(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 0, tx_nonce: 0, account_nonce: 0);
    let input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1, account_nonce: 0);

    // Test.
    add_tx(&mut mempool, &input_nonce_0);
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: only eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_0.tx, input_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();

    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_filling_hole(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 1, tx_nonce: 0, account_nonce: 0);
    let input_nonce_1 = add_tx_input!(tx_hash: 2, tx_nonce: 1, account_nonce: 0);

    // Test: add the second transaction first, which creates a hole in the sequence.
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: the second transaction is in the pool and not in the queue.
    let expected_pool_txs = [input_nonce_1.tx.clone()];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(expected_pool_txs).with_priority_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_priority_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

// `commit_block` tests.

#[rstest]
fn test_add_tx_after_get_txs_fails_on_duplicate_nonce() {
    // Setup.
    let input_tx = add_tx_input!(tx_hash: 0, tx_nonce: 0);
    let input_tx_duplicate_nonce = add_tx_input!(tx_hash: 1, tx_nonce: 0);

    let pool_txs = [input_tx.tx.clone()];
    let queue_txs = [TransactionReference::new(&input_tx.tx)];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    mempool.get_txs(1).unwrap();
    add_tx_expect_error(
        &mut mempool,
        &input_tx_duplicate_nonce,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(0) },
    );
}

#[rstest]
fn test_commit_block_includes_all_txs() {
    // Setup.
    let tx_address_0_nonce_4 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 4);
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5);
    let tx_address_1_nonce_3 = tx!(tx_hash: 3, sender_address: "0x1", tx_nonce: 3);
    let tx_address_2_nonce_1 = tx!(tx_hash: 4, sender_address: "0x2", tx_nonce: 1);

    let queue_txs = [&tx_address_0_nonce_4, &tx_address_1_nonce_3, &tx_address_2_nonce_1]
        .map(TransactionReference::new);
    let pool_txs =
        [tx_address_0_nonce_4, tx_address_0_nonce_5, tx_address_1_nonce_3, tx_address_2_nonce_1];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 3), ("0x1", 2)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_priority_queue(queue_txs).build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

#[rstest]
fn test_commit_block_rewinds_queued_nonce() {
    // Setup.
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5);

    let queued_txs = [TransactionReference::new(&tx_address_0_nonce_5)];
    let pool_txs = [tx_address_0_nonce_5];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 3), ("0x1", 3)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_mempool_content = MempoolContentBuilder::new().with_priority_queue([]).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
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
        tx_address_1_nonce_2,
    ];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [
        ("0x0", 5),
        // A hole, missing nonce 1 for address "0x1".
        ("0x1", 0),
        ("0x2", 1),
    ];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_queue_txs = [&tx_address_0_nonce_6].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}

// `account_nonces` tests.

#[rstest]
fn test_account_nonces_update_in_add_tx(mut mempool: Mempool) {
    // Setup.
    let input = add_tx_input!(tx_nonce: 1, account_nonce: 1);

    // Test: update through new input.
    add_tx(&mut mempool, &input);

    // Assert.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_account_nonces([("0x0", 1)]).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonce_does_not_decrease_in_add_tx() {
    // Setup.
    let input_with_lower_account_nonce = add_tx_input!(tx_nonce: 0, account_nonce: 0);
    let account_nonces = [("0x0", 2)];
    let mut mempool =
        MempoolContentBuilder::new().with_account_nonces(account_nonces).build_into_mempool();

    // Test: receives a transaction with a lower account nonce.
    add_tx(&mut mempool, &input_with_lower_account_nonce);

    // Assert: the account nonce is not updated.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_account_nonces(account_nonces).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonces_update_in_commit_block() {
    // Setup.
    let pool_txs = [tx!(tx_nonce: 2)];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_account_nonces([("0x0", 0)])
        .build_into_mempool();

    // Test: update through a commit block.
    let state_changes = [("0x0", 0)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_account_nonces = [("0x0", 1)]; // Account nonce advanced.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_account_nonces(expected_account_nonces).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonce_does_not_decrease_in_commit_block() {
    // Setup.
    let account_nonces = [("0x0", 2)];
    let pool_txs = [tx!(tx_nonce: 3)];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_account_nonces(account_nonces)
        .build_into_mempool();

    // Test: commits state change of a lower account nonce.
    let state_changes = [("0x0", 0)];
    commit_block(&mut mempool, state_changes);

    // Assert: the account nonce is not updated.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_account_nonces(account_nonces).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonces_removal_in_commit_block(mut mempool: Mempool) {
    // Test: commit block returns information about account that is not in the mempool.
    let state_changes = [("0x0", 0)];
    commit_block(&mut mempool, state_changes);

    // Assert: account is not added to the mempool.
    let expected_mempool_content = MempoolContentBuilder::new().with_account_nonces([]).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

// Flow tests.

#[rstest]
fn test_flow_filling_holes(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0, account_nonce: 0);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1, account_nonce: 0);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0, account_nonce: 0);

    add_tx(&mut mempool, &input_address_0_nonce_1);
    add_tx(&mut mempool, &input_address_1_nonce_0);

    // Test and assert: only the eligible transaction is returned.
    get_txs_and_assert_expected(&mut mempool, 2, &[input_address_1_nonce_0.tx]);

    add_tx(&mut mempool, &input_address_0_nonce_0);

    // Test and assert: all remaining transactions are returned.
    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[input_address_0_nonce_0.tx, input_address_0_nonce_1.tx],
    );
}

#[rstest]
fn test_flow_partial_commit_block() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_address_0_nonce_5 = tx!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 5);
    let tx_address_0_nonce_6 = tx!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 6);
    let tx_address_1_nonce_0 = tx!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 0);
    let tx_address_1_nonce_1 = tx!(tip: 21, tx_hash: 5, sender_address: "0x1", tx_nonce: 1);
    let tx_address_1_nonce_2 = tx!(tip: 22, tx_hash: 6, sender_address: "0x1", tx_nonce: 2);
    let tx_address_2_nonce_2 = tx!(tip: 0, tx_hash: 7, sender_address: "0x2", tx_nonce: 2);

    let queue_txs = [&tx_address_0_nonce_3, &tx_address_1_nonce_0, &tx_address_2_nonce_2]
        .map(TransactionReference::new);
    let pool_txs = [
        &tx_address_0_nonce_3,
        &tx_address_0_nonce_5,
        &tx_address_0_nonce_6,
        &tx_address_1_nonce_0,
        &tx_address_1_nonce_1,
        &tx_address_1_nonce_2,
        &tx_address_2_nonce_2,
    ]
    .map(|tx| tx.clone());
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_address_1_nonce_0, tx_address_0_nonce_3]);
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_address_1_nonce_1, tx_address_2_nonce_2]);

    // Not included in block: address "0x2" nonce 2, address "0x1" nonce 1.
    let state_changes = [("0x0", 3), ("0x1", 0)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_pool_txs = [tx_address_0_nonce_5, tx_address_0_nonce_6, tx_address_1_nonce_2];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(expected_pool_txs).with_priority_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_priority_queue_content(&mempool);
}

#[rstest]
fn test_flow_commit_block_closes_hole() {
    // Setup.
    let tx_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_input_nonce_4 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 4, account_nonce: 5);
    let tx_nonce_5 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 5);

    let queued_txs = [TransactionReference::new(&tx_nonce_3)];
    let pool_txs = [tx_nonce_3, tx_nonce_5.clone()];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 4)];
    commit_block(&mut mempool, state_changes);

    // Assert: hole was indeed closed.
    let expected_queue_txs = [&tx_nonce_5].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);

    add_tx_expect_error(
        &mut mempool,
        &tx_input_nonce_4,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(4) },
    );
}

#[rstest]
fn test_flow_send_same_nonce_tx_after_previous_not_included() {
    // Setup.
    let tx_nonce_3 = tx!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3);
    let tx_input_nonce_4 =
        add_tx_input!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 4, account_nonce: 4);
    let tx_nonce_5 = tx!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 5);

    let queue_txs = [TransactionReference::new(&tx_nonce_3)];
    let pool_txs = [&tx_nonce_3, &tx_input_nonce_4.tx, &tx_nonce_5].map(|tx| tx.clone());
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_priority_queue(queue_txs)
        .build_into_mempool();

    // Test.
    get_txs_and_assert_expected(&mut mempool, 2, &[tx_nonce_3, tx_input_nonce_4.tx.clone()]);

    let state_changes = [("0x0", 3)]; // Transaction with nonce 4 is not included in the block.
    commit_block(&mut mempool, state_changes);

    add_tx(&mut mempool, &tx_input_nonce_4);

    get_txs_and_assert_expected(&mut mempool, 1, &[tx_input_nonce_4.tx]);

    // Assert.
    let expected_queue_txs = [TransactionReference::new(&tx_nonce_5)];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_priority_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_tx_priority_queue_content(&mempool);
}
