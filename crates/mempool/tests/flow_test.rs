use mempool_test_utils::starknet_api_test_utils::test_resource_bounds_mapping;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_api::{contract_address, felt, invoke_tx_args, nonce, patricia_key};
use starknet_mempool::mempool::Mempool;
use starknet_mempool::test_utils::{add_tx, commit_block, get_txs_and_assert_expected};
use starknet_mempool::{add_tx_input, tx};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// Tests.

#[rstest]
fn test_add_tx_fills_nonce_gap(mut mempool: Mempool) {
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
fn test_commit_block_includes_proposed_txs_subset(mut mempool: Mempool) {
    // Setup.
    let tx_address_0_nonce_3 =
        add_tx_input!(tx_hash: 1, sender_address: "0x0", tx_nonce: 3, account_nonce: 3);
    let tx_address_0_nonce_5 =
        add_tx_input!(tx_hash: 2, sender_address: "0x0", tx_nonce: 5, account_nonce: 3);
    let tx_address_0_nonce_6 =
        add_tx_input!(tx_hash: 3, sender_address: "0x0", tx_nonce: 6, account_nonce: 3);
    let tx_address_1_nonce_0 =
        add_tx_input!(tx_hash: 4, sender_address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx_address_1_nonce_1 =
        add_tx_input!(tx_hash: 5, sender_address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx_address_1_nonce_2 =
        add_tx_input!(tx_hash: 6, sender_address: "0x1", tx_nonce: 2, account_nonce: 0);
    let tx_address_2_nonce_2 =
        add_tx_input!(tx_hash: 7, sender_address: "0x2", tx_nonce: 2, account_nonce: 2);

    for input in [
        &tx_address_0_nonce_5,
        &tx_address_0_nonce_6,
        &tx_address_0_nonce_3,
        &tx_address_1_nonce_2,
        &tx_address_1_nonce_1,
        &tx_address_1_nonce_0,
        &tx_address_2_nonce_2,
    ] {
        add_tx(&mut mempool, input);
    }

    // Test.
    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_address_2_nonce_2.tx, tx_address_1_nonce_0.tx],
    );
    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_address_1_nonce_1.tx, tx_address_0_nonce_3.tx],
    );

    // Not included in block: address "0x2" nonce 2, address "0x1" nonce 1.
    let nonces = [("0x0", 3), ("0x1", 0)];
    commit_block(&mut mempool, nonces);

    get_txs_and_assert_expected(&mut mempool, 2, &[]);
}
