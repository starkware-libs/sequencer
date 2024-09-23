use std::cmp::Reverse;
use std::collections::HashMap;

use assert_matches::assert_matches;
use mempool_test_utils::starknet_api_test_utils::test_resource_bounds_mapping;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_api::{contract_address, felt, invoke_tx_args, patricia_key};
use starknet_mempool_types::errors::MempoolError;
use starknet_mempool_types::mempool_types::{Account, AccountState};
use starknet_types_core::felt::Felt;

use crate::mempool::{AccountToNonce, Mempool, MempoolInput, TransactionReference};
use crate::transaction_pool::TransactionPool;
use crate::transaction_queue::TransactionQueue;

// Utils.

/// Represents the internal content of the mempool.
/// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct MempoolContent {
    tx_pool: Option<TransactionPool>,
    tx_queue: Option<TransactionQueue>,
    account_nonces: Option<AccountToNonce>,
}

impl MempoolContent {
    fn assert_eq_pool_and_queue_content(&self, mempool: &Mempool) {
        self.assert_eq_transaction_pool_content(mempool);
        self.assert_eq_transaction_queue_content(mempool);
    }

    fn assert_eq_transaction_pool_content(&self, mempool: &Mempool) {
        assert_eq!(self.tx_pool.as_ref().unwrap(), &mempool.tx_pool);
    }

    fn assert_eq_transaction_queue_content(&self, mempool: &Mempool) {
        assert_eq!(self.tx_queue.as_ref().unwrap(), &mempool.tx_queue);
    }

    fn assert_eq_account_nonces(&self, mempool: &Mempool) {
        assert_eq!(self.account_nonces.as_ref().unwrap(), &mempool.account_nonces);
    }
}

impl From<MempoolContent> for Mempool {
    fn from(mempool_content: MempoolContent) -> Mempool {
        let MempoolContent { tx_pool, tx_queue, account_nonces } = mempool_content;
        Mempool {
            tx_pool: tx_pool.unwrap_or_default(),
            tx_queue: tx_queue.unwrap_or_default(),
            // TODO: Add implementation when needed.
            mempool_state: Default::default(),
            account_nonces: account_nonces.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
struct MempoolContentBuilder {
    tx_pool: Option<TransactionPool>,
    tx_queue: Option<TransactionQueue>,
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

    fn with_queue<Q>(mut self, queue_txs: Q) -> Self
    where
        Q: IntoIterator<Item = TransactionReference>,
    {
        self.tx_queue = Some(queue_txs.into_iter().collect());
        self
    }

    fn with_account_nonces<A>(mut self, account_nonce_pairs: A) -> Self
    where
        A: IntoIterator<Item = (ContractAddress, Nonce)>,
    {
        self.account_nonces = Some(account_nonce_pairs.into_iter().collect());
        self
    }

    fn build(self) -> MempoolContent {
        MempoolContent {
            tx_pool: self.tx_pool,
            tx_queue: self.tx_queue,
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
    let state_changes = HashMap::from_iter(state_changes.into_iter().map(|(address, nonce)| {
        (contract_address!(address), AccountState { nonce: Nonce(felt!(nonce)) })
    }));

    assert_eq!(mempool.commit_block(state_changes), Ok(()));
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
                nonce: Nonce(felt!($tx_nonce)),
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
        tx!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0_u8)
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
        let sender_address = contract_address!($sender_address);
        let account_nonce = Nonce(felt!($account_nonce));
        let account = Account { sender_address, state: AccountState {nonce: account_nonce}};

        MempoolInput { tx, account }
    }};
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr,
        tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {{
            add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce, resource_bounds: ValidResourceBounds::AllResources(test_resource_bounds_mapping()))
    }};
    (tx_hash: $tx_hash:expr, sender_address: $sender_address:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr, sender_address: $sender_address:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: $sender_address, tx_nonce: 0_u8, account_nonce: 0_u8)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tx_nonce: $tx_nonce:expr, account_nonce: $account_nonce:expr) => {
        add_tx_input!(tip: 1, tx_hash: 0_u8, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: $account_nonce)
    };
    (tip: $tip:expr, tx_hash: $tx_hash:expr) => {
        add_tx_input!(tip: $tip, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8)
    };
    (tx_hash: $tx_hash:expr, tx_nonce: $tx_nonce:expr) => {
        add_tx_input!(tip: 0, tx_hash: $tx_hash, sender_address: "0x0", tx_nonce: $tx_nonce, account_nonce: 0_u8)
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
fn test_get_txs_returns_by_priority_order(#[case] requested_txs: usize) {
    // Setup.
    let mut txs = [
        tx!(tip: 20, tx_hash: 1, sender_address: "0x0"),
        tx!(tip: 30, tx_hash: 2, sender_address: "0x1"),
        tx!(tip: 10, tx_hash: 3, sender_address: "0x2"),
    ];

    let tx_references_iterator = txs.iter().map(TransactionReference::new);
    let txs_iterator = txs.iter().cloned();
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(txs_iterator)
        .with_queue(tx_references_iterator)
        .build_into_mempool();

    // Test.
    let fetched_txs = mempool.get_txs(requested_txs).unwrap();

    txs.sort_by_key(|tx| Reverse(tx.tip()));

    // Ensure we do not exceed the number of transactions available in the mempool.
    let max_requested_txs = requested_txs.min(txs.len());

    // Check that the returned transactions are the ones with the highest priority.
    let (expected_queue, remaining_txs) = txs.split_at(max_requested_txs);
    assert_eq!(fetched_txs, expected_queue);

    // Assert: non-returned transactions are still in the mempool.
    let remaining_tx_references = remaining_txs.iter().map(TransactionReference::new);
    let mempool_content = MempoolContentBuilder::new()
        .with_pool(remaining_txs.to_vec())
        .with_queue(remaining_tx_references)
        .build();
    mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_multi_nonce() {
    // Setup.
    let tx_nonce_0 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8);
    let tx_nonce_1 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8);
    let tx_nonce_2 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 2_u8);

    let queue_txs = [&tx_nonce_0].map(TransactionReference::new);
    let pool_txs = [tx_nonce_0, tx_nonce_1, tx_nonce_2];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_queue(queue_txs)
        .build_into_mempool();

    // Test.
    let fetched_txs = mempool.get_txs(3).unwrap();

    // Assert: all transactions are returned.
    assert_eq!(fetched_txs, &pool_txs);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool([]).with_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_only_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 = tx!(tip: 20, tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8);
    let tx_address_0_nonce_1 = tx!(tip: 20, tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8);
    let tx_address_1_nonce_0 = tx!(tip: 10, tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8);

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs =
        [&tx_address_0_nonce_0, &tx_address_0_nonce_1, &tx_address_1_nonce_0].map(|tx| tx.clone());
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    let txs = mempool.get_txs(3).unwrap();

    // Assert: all transactions returned.
    // Replenishment done in chunks: account 1 transaction is returned before the one of account 0,
    // although its priority is higher.
    assert_eq!(txs, &[tx_address_0_nonce_0, tx_address_1_nonce_0, tx_address_0_nonce_1]);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool([]).with_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_replenishes_queue_multi_account_between_chunks() {
    // Setup.
    let tx_address_0_nonce_0 = tx!(tip: 30, tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8);
    let tx_address_0_nonce_1 = tx!(tip: 30, tx_hash: 3, sender_address: "0x0", tx_nonce: 1_u8);
    let tx_address_1_nonce_0 = tx!(tip: 20, tx_hash: 2, sender_address: "0x1", tx_nonce: 0_u8);
    let tx_address_1_nonce_1 = tx!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 1_u8);

    let queue_txs = [&tx_address_0_nonce_0, &tx_address_1_nonce_0].map(TransactionReference::new);
    let pool_txs = [
        &tx_address_0_nonce_0,
        &tx_address_1_nonce_0,
        &tx_address_0_nonce_1,
        &tx_address_1_nonce_1,
    ]
    .map(|tx| tx.clone());
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert.
    assert_eq!(txs, [tx_address_0_nonce_0, tx_address_1_nonce_0]);

    // Queue is replenished with the next transactions of each account.
    let expected_queue_txs =
        [&tx_address_0_nonce_1, &tx_address_1_nonce_1].map(TransactionReference::new);
    let expected_pool_txs = [tx_address_0_nonce_1, tx_address_1_nonce_1];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_with_holes_multiple_accounts() {
    // Setup.
    let tx_address_0_nonce_1 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8);
    let tx_address_1_nonce_0 = tx!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8);

    let queue_txs = [TransactionReference::new(&tx_address_1_nonce_0)];
    let pool_txs = [tx_address_0_nonce_1.clone(), tx_address_1_nonce_0.clone()];
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    let txs = mempool.get_txs(2).unwrap();

    // Assert.
    assert_eq!(txs, &[tx_address_1_nonce_0]);

    let expected_pool_txs = [tx_address_0_nonce_1];
    let expected_queue_txs = [];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_with_holes_single_account() {
    // Setup.
    let pool_txs = [tx!(tx_nonce: 1_u8)];
    let queue_txs = [];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_queue(queue_txs)
        .build_into_mempool();

    // Test.
    let txs = mempool.get_txs(1).unwrap();

    // Assert.
    assert_eq!(txs, &[]);

    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_get_txs_while_decreasing_gas_price_threshold() {
    // Setup.
    let tx = tx!(tx_nonce: 0_u8);

    let queue_txs = [TransactionReference::new(&tx)];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool([tx.clone()])
        .with_queue(queue_txs)
        .build_into_mempool();

    // Test.
    // High gas price threshold, no transactions should be returned.
    mempool._update_gas_price_threshold(1000000000000);
    let txs = mempool.get_txs(1).unwrap();
    assert!(txs.is_empty());

    // Updating the gas price threshold should happen in a new block creation.
    let state_changes = [];
    commit_block(&mut mempool, state_changes);

    // Low gas price threshold, the transaction should be returned.
    mempool._update_gas_price_threshold(100);
    let txs = mempool.get_txs(1).unwrap();
    assert_eq!(txs, &[tx]);
}

#[rstest]
fn test_get_txs_while_increasing_gas_price_threshold() {
    // Setup.
    // Both transactions have the same gas price.
    let tx_nonce_0 = tx!(tx_hash: 0, tx_nonce: 0_u8);
    let tx_nonce_1 = tx!(tx_hash: 1, tx_nonce: 1_u8);

    let queue_txs = [TransactionReference::new(&tx_nonce_0)];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool([tx_nonce_0.clone(), tx_nonce_1])
        .with_queue(queue_txs)
        .build_into_mempool();

    // Test.
    // Low gas price threshold, the transaction should be returned.
    mempool._update_gas_price_threshold(100);
    let txs = mempool.get_txs(1).unwrap();
    assert_eq!(txs, &[tx_nonce_0]);

    // Updating the gas price threshold should happen in a new block creation.
    let state_changes = [];
    commit_block(&mut mempool, state_changes);

    // High gas price threshold, no transactions should be returned.
    mempool._update_gas_price_threshold(1000000000000);
    let txs = mempool.get_txs(1).unwrap();
    assert!(txs.is_empty());
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
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_multi_nonce_success(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 2, sender_address: "0x1", tx_nonce: 0_u8,account_nonce: 0_u8);

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
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
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
    expected_mempool_content.assert_eq_transaction_pool_content(&mempool);
}

#[rstest]
fn test_add_tx_lower_than_queued_nonce() {
    // Setup.
    let valid_input =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);
    let lower_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);

    let MempoolInput {
        tx: valid_input_tx,
        account: Account { sender_address, state: AccountState { nonce } },
    } = valid_input;
    let queue_txs = [TransactionReference::new(&valid_input_tx)];
    let account_nonces = [(sender_address, nonce)];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_queue(queue_txs)
        .with_account_nonces(account_nonces)
        .build();

    let pool_txs = [valid_input_tx];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_queue(queue_txs)
        .with_account_nonces(account_nonces)
        .build_into_mempool();

    // Test and assert the original transaction remains.
    add_tx_expect_error(
        &mut mempool,
        &lower_nonce_input,
        MempoolError::DuplicateNonce {
            address: contract_address!("0x0"),
            nonce: Nonce(felt!(0_u16)),
        },
    );
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_add_tx_updates_queue_with_higher_account_nonce() {
    // Setup.
    let input =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let higher_account_nonce_input =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);

    let queue_txs = [TransactionReference::new(&input.tx)];
    let mut mempool = MempoolContentBuilder::new().with_queue(queue_txs).build_into_mempool();

    // Test.
    add_tx(&mut mempool, &higher_account_nonce_input);

    // Assert: the higher account nonce transaction is in the queue.
    let expected_queue_txs = [TransactionReference::new(&higher_account_nonce_input.tx)];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
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
        .with_queue(expected_queue_txs)
        .build();

    // TODO: currently hash comparison tie-breaks the two. Once more robust tie-breaks are added
    // replace this assertion with a dedicated test.
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_delete_tx_with_lower_nonce_than_account_nonce() {
    // Setup.
    let tx_nonce_0_account_nonce_0 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let tx_nonce_1_account_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 1_u8);

    let queue_txs = [TransactionReference::new(&tx_nonce_0_account_nonce_0.tx)];
    let pool_txs = [tx_nonce_0_account_nonce_0.tx];
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    add_tx(&mut mempool, &tx_nonce_1_account_nonce_1);

    // Assert the transaction with the lower nonce is removed.
    let expected_queue_txs = [TransactionReference::new(&tx_nonce_1_account_nonce_1.tx)];
    let expected_pool_txs = [tx_nonce_1_account_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
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
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_account_state_fills_hole(mut mempool: Mempool) {
    // Setup.
    let tx_input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1_u8, account_nonce: 0_u8);
    // Input that increments the account state.
    let tx_input_nonce_2 = add_tx_input!(tx_hash: 2, tx_nonce: 2_u8, account_nonce: 1_u8);

    // Test and assert.

    // First, with gap.
    add_tx(&mut mempool, &tx_input_nonce_1);
    let expected_mempool_content = MempoolContentBuilder::new().with_queue([]).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);

    // Then, fill it.
    add_tx(&mut mempool, &tx_input_nonce_2);
    let expected_queue_txs = [&tx_input_nonce_1.tx].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_sequential_nonces(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 0, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_nonce_1 = add_tx_input!(tx_hash: 1, tx_nonce: 1_u8, account_nonce: 0_u8);

    // Test.
    add_tx(&mut mempool, &input_nonce_0);
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: only eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_0.tx, input_nonce_1.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();

    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_add_tx_filling_hole(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 1, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_nonce_1 = add_tx_input!(tx_hash: 2, tx_nonce: 1_u8, account_nonce: 0_u8);

    // Test: add the second transaction first, which creates a hole in the sequence.
    add_tx(&mut mempool, &input_nonce_1);

    // Assert: the second transaction is in the pool and not in the queue.
    let expected_queue_txs = [];
    let expected_pool_txs = [input_nonce_1.tx.clone()];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);

    // Test: add the first transaction, which fills the hole.
    add_tx(&mut mempool, &input_nonce_0);

    // Assert: only the eligible transaction appears in the queue.
    let expected_queue_txs = [TransactionReference::new(&input_nonce_0.tx)];
    let expected_pool_txs = [input_nonce_1.tx, input_nonce_0.tx];
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_pool(expected_pool_txs)
        .with_queue(expected_queue_txs)
        .build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

// `commit_block` tests.

#[rstest]
fn test_add_tx_after_get_txs_fails_on_duplicate_nonce() {
    // Setup.
    let input_tx = add_tx_input!(tx_hash: 0, tx_nonce: 0_u8);
    let input_tx_duplicate_nonce = add_tx_input!(tx_hash: 1, tx_nonce: 0_u8);

    let pool_txs = [input_tx.tx.clone()];
    let queue_txs = [TransactionReference::new(&input_tx.tx)];
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    mempool.get_txs(1).unwrap();
    add_tx_expect_error(
        &mut mempool,
        &input_tx_duplicate_nonce,
        MempoolError::DuplicateNonce {
            address: contract_address!("0x0"),
            nonce: Nonce(felt!(0_u16)),
        },
    );
}

#[rstest]
fn test_commit_block_includes_all_txs() {
    // Setup.
    let tx_address_0_nonce_4 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 4_u8);
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8);
    let tx_address_1_nonce_3 = tx!(tx_hash: 3, sender_address: "0x1", tx_nonce: 3_u8);
    let tx_address_2_nonce_1 = tx!(tx_hash: 4, sender_address: "0x2", tx_nonce: 1_u8);

    let queue_txs = [&tx_address_0_nonce_4, &tx_address_1_nonce_3, &tx_address_2_nonce_1]
        .map(TransactionReference::new);
    let pool_txs =
        [tx_address_0_nonce_4, tx_address_0_nonce_5, tx_address_1_nonce_3, tx_address_2_nonce_1];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs.clone())
        .with_queue(queue_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 3_u8), ("0x1", 2_u8)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_commit_block_rewinds_nonce() {
    // Setup.
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8);

    let queued_txs = [TransactionReference::new(&tx_address_0_nonce_5)];
    let pool_txs = [tx_address_0_nonce_5];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 3_u8), ("0x1", 3_u8)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_mempool_content = MempoolContentBuilder::new().with_queue([]).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
}

#[rstest]
fn test_commit_block_from_different_leader() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8);
    let tx_address_0_nonce_5 = tx!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8);
    let tx_address_0_nonce_6 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 6_u8);
    let tx_address_1_nonce_2 = tx!(tx_hash: 4, sender_address: "0x1", tx_nonce: 2_u8);

    let queued_txs = [TransactionReference::new(&tx_address_1_nonce_2)];
    let pool_txs = [
        tx_address_0_nonce_3,
        tx_address_0_nonce_5,
        tx_address_0_nonce_6.clone(),
        tx_address_1_nonce_2,
    ];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [
        ("0x0", 5_u8),
        // A hole, missing nonce 1 for address "0x1".
        ("0x1", 0_u8),
        ("0x2", 1_u8),
    ];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_queue_txs = [&tx_address_0_nonce_6].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
}

// `account_nonces` tests.

#[rstest]
fn test_account_nonces_update_in_add_tx(mut mempool: Mempool) {
    // Setup.
    let input = add_tx_input!(tx_nonce: 1_u8, account_nonce: 1_u8);

    // Test: update through new input.
    add_tx(&mut mempool, &input);

    // Assert.
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_account_nonces([(input.account.sender_address, input.account.state.nonce)])
        .build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonce_does_not_decrease_in_add_tx() {
    // Setup.
    let input_with_lower_account_nonce = add_tx_input!(tx_nonce: 0_u8, account_nonce: 0_u8);
    let account_nonces =
        [(input_with_lower_account_nonce.account.sender_address, Nonce(felt!(2_u8)))];
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
    let input = add_tx_input!(tx_nonce: 2_u8, account_nonce: 0_u8);
    let Account { sender_address, state: AccountState { nonce } } = input.account;
    let pool_txs = [input.tx];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_account_nonces([(sender_address, nonce)])
        .build_into_mempool();
    let committed_nonce = Nonce(Felt::ZERO);

    // Test: update through a commit block.
    let state_changes = HashMap::from([(sender_address, AccountState { nonce: committed_nonce })]);
    assert_eq!(mempool.commit_block(state_changes), Ok(()));

    // Assert.
    let expected_mempool_content = MempoolContentBuilder::new()
        .with_account_nonces([(sender_address, committed_nonce.try_increment().unwrap())])
        .build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonce_does_not_decrease_in_commit_block() {
    // Setup.
    let input_account_nonce_2 = add_tx_input!(tx_nonce: 3_u8, account_nonce: 2_u8);
    let Account { sender_address, state: AccountState { nonce } } = input_account_nonce_2.account;
    let account_nonces = [(sender_address, nonce)];
    let pool_txs = [input_account_nonce_2.tx];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_account_nonces(account_nonces)
        .build_into_mempool();

    // Test: commits state change of a lower account nonce.
    let state_changes =
        HashMap::from([(sender_address, AccountState { nonce: Nonce(felt!(0_u8)) })]);
    assert_eq!(mempool.commit_block(state_changes), Ok(()));

    // Assert: the account nonce is not updated.
    let expected_mempool_content =
        MempoolContentBuilder::new().with_account_nonces(account_nonces).build();
    expected_mempool_content.assert_eq_account_nonces(&mempool);
}

#[rstest]
fn test_account_nonces_removal_in_commit_block(mut mempool: Mempool) {
    // Test: commit block returns information about account that is not in the mempool.
    let state_changes = [("0x0", 0_u8)];
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
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 1_u8, account_nonce: 0_u8);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 3, sender_address: "0x1", tx_nonce: 0_u8, account_nonce: 0_u8);

    // Test.
    add_tx(&mut mempool, &input_address_0_nonce_1);
    add_tx(&mut mempool, &input_address_1_nonce_0);
    let txs = mempool.get_txs(2).unwrap();

    // Assert: only the eligible transaction is returned.
    assert_eq!(txs, &[input_address_1_nonce_0.tx]);

    // Test.
    add_tx(&mut mempool, &input_address_0_nonce_0);
    let txs = mempool.get_txs(2).unwrap();

    // Assert: all remaining transactions are returned.
    assert_eq!(txs, &[input_address_0_nonce_0.tx, input_address_0_nonce_1.tx]);
}

#[rstest]
fn test_flow_partial_commit_block() {
    // Setup.
    let tx_address_0_nonce_3 = tx!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8);
    let tx_address_0_nonce_5 = tx!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 5_u8);
    let tx_address_0_nonce_6 = tx!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 6_u8);
    let tx_address_1_nonce_0 = tx!(tip: 20, tx_hash: 4, sender_address: "0x1", tx_nonce: 0_u8);
    let tx_address_1_nonce_1 = tx!(tip: 21, tx_hash: 5, sender_address: "0x1", tx_nonce: 1_u8);
    let tx_address_1_nonce_2 = tx!(tip: 22, tx_hash: 6, sender_address: "0x1", tx_nonce: 2_u8);
    let tx_address_2_nonce_2 = tx!(tip: 0, tx_hash: 7, sender_address: "0x2", tx_nonce: 2_u8);

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
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.

    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_address_1_nonce_0, tx_address_0_nonce_3]);

    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_address_1_nonce_1, tx_address_2_nonce_2]);

    // Not included in block: address "0x2" nonce 2, address "0x1" nonce 1.
    let state_changes = [("0x0", 3_u8), ("0x1", 0_u8)];
    commit_block(&mut mempool, state_changes);

    // Assert.
    let expected_pool_txs = [tx_address_0_nonce_5, tx_address_0_nonce_6, tx_address_1_nonce_2];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_pool(expected_pool_txs).with_queue([]).build();
    expected_mempool_content.assert_eq_pool_and_queue_content(&mempool);
}

#[rstest]
fn test_flow_commit_block_closes_hole() {
    // Setup.
    let tx_nonce_3 = tx!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8);
    let tx_input_nonce_4 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 4_u8, account_nonce: 5_u8);
    let tx_nonce_5 = tx!(tx_hash: 3, sender_address: "0x0", tx_nonce: 5_u8);

    let queued_txs = [TransactionReference::new(&tx_nonce_3)];
    let pool_txs = [tx_nonce_3, tx_nonce_5.clone()];
    let mut mempool = MempoolContentBuilder::new()
        .with_pool(pool_txs)
        .with_queue(queued_txs)
        .build_into_mempool();

    // Test.
    let state_changes = [("0x0", 4_u8)];
    commit_block(&mut mempool, state_changes);

    // Assert: hole was indeed closed.
    let expected_queue_txs = [&tx_nonce_5].map(TransactionReference::new);
    let expected_mempool_content =
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);

    add_tx_expect_error(
        &mut mempool,
        &tx_input_nonce_4,
        MempoolError::DuplicateNonce {
            address: contract_address!("0x0"),
            nonce: Nonce(felt!(4_u8)),
        },
    );
}

#[rstest]
fn test_flow_send_same_nonce_tx_after_previous_not_included() {
    // Setup.
    let tx_nonce_3 = tx!(tip: 10, tx_hash: 1, sender_address: "0x0", tx_nonce: 3_u8);
    let tx_input_nonce_4 = add_tx_input!(tip: 11, tx_hash: 2, sender_address: "0x0", tx_nonce: 4_u8, account_nonce: 4_u8);
    let tx_nonce_5 = tx!(tip: 12, tx_hash: 3, sender_address: "0x0", tx_nonce: 5_u8);

    let queue_txs = [TransactionReference::new(&tx_nonce_3)];
    let pool_txs = [&tx_nonce_3, &tx_input_nonce_4.tx, &tx_nonce_5].map(|tx| tx.clone());
    let mut mempool =
        MempoolContentBuilder::new().with_pool(pool_txs).with_queue(queue_txs).build_into_mempool();

    // Test.
    let txs = mempool.get_txs(2).unwrap();
    assert_eq!(txs, &[tx_nonce_3, tx_input_nonce_4.tx.clone()]);

    // Transaction with nonce 4 is not included in the block.
    let state_changes = [("0x0", 3_u8)];
    commit_block(&mut mempool, state_changes);

    add_tx(&mut mempool, &tx_input_nonce_4);
    let txs = mempool.get_txs(1).unwrap();

    // Assert.
    assert_eq!(txs, &[tx_input_nonce_4.tx]);
    let expected_queue_txs = [TransactionReference::new(&tx_nonce_5)];
    let expected_mempool_content =
        MempoolContentBuilder::new().with_queue(expected_queue_txs).build();
    expected_mempool_content.assert_eq_transaction_queue_content(&mempool);
}

#[rstest]
fn test_tx_pool_capacity(mut mempool: Mempool) {
    let input_1 =
        add_tx_input!(tx_hash: 0, sender_address: 0_u8, tx_nonce: 0_u8, account_nonce: 0_u8);
    let input_2 =
        add_tx_input!(tx_hash: 1, sender_address: 1_u8, tx_nonce: 0_u8, account_nonce: 0_u8);

    // Test and assert: add txs to the counter.
    add_tx(&mut mempool, &input_1);
    add_tx(&mut mempool, &input_2);
    assert_eq!(mempool.tx_pool().n_txs(), 2);

    // Test and assert: duplicate transaction doesn't affect capacity.
    add_tx_expect_error(
        &mut mempool,
        &input_1,
        MempoolError::DuplicateTransaction { tx_hash: input_1.tx.tx_hash() },
    );
    assert_eq!(mempool.tx_pool().n_txs(), 2);

    // Test and assert: updates pool capacity when a transaction is removed upon receiving state
    // changes.
    let state_changes = [("0x0", 4_u8)];
    commit_block(&mut mempool, state_changes);
    assert_eq!(mempool.tx_pool().n_txs(), 1);

    // Test and assert: remove the transactions, counter does not go below 0.
    mempool.get_txs(2).unwrap();
    assert_eq!(mempool.tx_pool().n_txs(), 0);
}
