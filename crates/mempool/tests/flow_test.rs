use mempool_test_utils::starknet_api_test_utils::test_resource_bounds_mapping;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::{Tip, TransactionHash, ValidResourceBounds};
use starknet_api::{contract_address, felt, invoke_tx_args, nonce, patricia_key};
use starknet_mempool::mempool::Mempool;
use starknet_mempool::test_utils::{add_tx, get_txs_and_assert_expected};
use starknet_mempool::{add_tx_input, tx};
use starknet_mempool_types::mempool_types::{AccountState, AddTransactionArgs};

// Fixtures.

#[fixture]
fn mempool() -> Mempool {
    Mempool::empty()
}

// Tests.

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
