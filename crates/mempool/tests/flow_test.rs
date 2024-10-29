use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::{contract_address, invoke_tx_args, nonce, patricia_key};
use starknet_mempool::add_tx_input;
use starknet_mempool::mempool::Mempool;
use starknet_mempool::test_utils::{
    add_tx,
    add_tx_expect_error,
    commit_block,
    get_txs_and_assert_expected,
};
use starknet_mempool_types::errors::MempoolError;

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    Mempool::default()
}

// Tests.

#[rstest]
fn test_add_tx_fills_nonce_gap(mut mempool: Mempool) {
    // Setup.
    let input_address_0_nonce_0 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    let input_address_0_nonce_1 =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, account_nonce: 0);
    let input_address_1_nonce_0 =
        add_tx_input!(tx_hash: 3, address: "0x1", tx_nonce: 0, account_nonce: 0);

    for input in [&input_address_0_nonce_1, &input_address_1_nonce_0] {
        add_tx(&mut mempool, input);
    }

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
fn test_add_tx_after_get_txs_fails_on_duplicate_nonce(mut mempool: Mempool) {
    // Setup.
    let input_tx = add_tx_input!(tx_hash: 0, tx_nonce: 0);

    // Test.
    add_tx(&mut mempool, &input_tx);
    get_txs_and_assert_expected(&mut mempool, 1, &[input_tx.tx]);

    let input_tx_duplicate_nonce = add_tx_input!(tx_hash: 1, tx_nonce: 0);
    add_tx_expect_error(
        &mut mempool,
        &input_tx_duplicate_nonce,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(0) },
    );
}

#[rstest]
fn test_add_same_nonce_tx_after_previous_not_included_in_block(mut mempool: Mempool) {
    // Setup.
    let tx_nonce_3_account_nonce_3 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 3, account_nonce: 3);
    let tx_nonce_4_account_nonce_3 =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 4, account_nonce: 3);
    let tx_nonce_5_account_nonce_3 =
        add_tx_input!(tx_hash: 3, address: "0x0", tx_nonce: 5, account_nonce: 3);

    for input in
        [&tx_nonce_3_account_nonce_3, &tx_nonce_4_account_nonce_3, &tx_nonce_5_account_nonce_3]
    {
        add_tx(&mut mempool, input);
    }

    // Test.
    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_nonce_3_account_nonce_3.tx, tx_nonce_4_account_nonce_3.tx.clone()],
    );

    let nonces = [("0x0", 3)]; // Transaction with nonce 4 is not included in the block.
    let tx_hashes = [1];
    commit_block(&mut mempool, nonces, tx_hashes);

    let tx_nonce_4_account_nonce_4 =
        add_tx_input!(tx_hash: 4, address: "0x0", tx_nonce: 4, account_nonce: 4);
    add_tx_expect_error(
        &mut mempool,
        &tx_nonce_4_account_nonce_4,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(4) },
    );

    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_nonce_4_account_nonce_3.tx, tx_nonce_5_account_nonce_3.tx],
    );
}

#[rstest]
fn test_add_tx_considers_already_given_nonce(mut mempool: Mempool) {
    // Setup.
    let input_nonce_0 = add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 0, account_nonce: 0);
    let input_nonce_1 = add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 1, account_nonce: 0);

    // Test.
    add_tx(&mut mempool, &input_nonce_0);
    get_txs_and_assert_expected(&mut mempool, 1, &[input_nonce_0.tx]);
    add_tx(&mut mempool, &input_nonce_1);
    get_txs_and_assert_expected(&mut mempool, 1, &[input_nonce_1.tx]);
}

#[rstest]
fn test_commit_block_includes_proposed_txs_subset(mut mempool: Mempool) {
    // Setup.
    let tx_address_0_nonce_3 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 3, account_nonce: 3);
    let tx_address_0_nonce_5 =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 5, account_nonce: 3);
    let tx_address_0_nonce_6 =
        add_tx_input!(tx_hash: 3, address: "0x0", tx_nonce: 6, account_nonce: 3);
    let tx_address_1_nonce_0 =
        add_tx_input!(tx_hash: 4, address: "0x1", tx_nonce: 0, account_nonce: 0);
    let tx_address_1_nonce_1 =
        add_tx_input!(tx_hash: 5, address: "0x1", tx_nonce: 1, account_nonce: 0);
    let tx_address_1_nonce_2 =
        add_tx_input!(tx_hash: 6, address: "0x1", tx_nonce: 2, account_nonce: 0);
    let tx_address_2_nonce_2 =
        add_tx_input!(tx_hash: 7, address: "0x2", tx_nonce: 2, account_nonce: 2);

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
        &[tx_address_2_nonce_2.tx.clone(), tx_address_1_nonce_0.tx],
    );
    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_address_1_nonce_1.tx.clone(), tx_address_0_nonce_3.tx],
    );

    // Not included in block: address "0x2" nonce 2, address "0x1" nonce 1.
    let nonces = [("0x0", 3), ("0x1", 0)];
    let tx_hashes = [1, 4];
    commit_block(&mut mempool, nonces, tx_hashes);

    get_txs_and_assert_expected(
        &mut mempool,
        2,
        &[tx_address_2_nonce_2.tx, tx_address_1_nonce_1.tx],
    );
}

#[rstest]
fn test_commit_block_fills_nonce_gap(mut mempool: Mempool) {
    // Setup.
    let tx_nonce_3_account_nonce_3 =
        add_tx_input!(tx_hash: 1, address: "0x0", tx_nonce: 3, account_nonce: 3);
    let tx_nonce_5_account_nonce_3 =
        add_tx_input!(tx_hash: 2, address: "0x0", tx_nonce: 5, account_nonce: 3);

    // Test.
    for input in [&tx_nonce_3_account_nonce_3, &tx_nonce_5_account_nonce_3] {
        add_tx(&mut mempool, input);
    }

    get_txs_and_assert_expected(&mut mempool, 2, &[tx_nonce_3_account_nonce_3.tx]);

    let nonces = [("0x0", 4)];
    let tx_hashes = [1, 3];
    commit_block(&mut mempool, nonces, tx_hashes);

    // Assert: hole was indeed closed.
    let tx_nonce_4_account_nonce_4 =
        add_tx_input!(tx_hash: 3, address: "0x0", tx_nonce: 4, account_nonce: 4);
    add_tx_expect_error(
        &mut mempool,
        &tx_nonce_4_account_nonce_4,
        MempoolError::DuplicateNonce { address: contract_address!("0x0"), nonce: nonce!(4) },
    );

    get_txs_and_assert_expected(&mut mempool, 2, &[tx_nonce_5_account_nonce_3.tx]);
}
