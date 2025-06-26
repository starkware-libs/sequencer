use std::sync::Arc;

use apollo_time::test_utils::FakeClock;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::{contract_address, nonce, tx_hash};

use crate::transaction_pool::TransactionPool;
use crate::tx;

#[rstest]
fn test_get_lowest_nonce_tx() {
    let mut pool = TransactionPool::new(Arc::new(FakeClock::default()));

    let tx_address_0_nonce_1 = tx!(tx_hash: 1, address: "0x0", tx_nonce: 1);
    let tx_address_0_nonce_3 = tx!(tx_hash: 2, address: "0x0", tx_nonce: 3);
    let tx_address_1_nonce_0 = tx!(tx_hash: 3, address: "0x1", tx_nonce: 0);

    pool.insert(tx_address_0_nonce_3).unwrap();
    pool.insert(tx_address_0_nonce_1).unwrap();
    pool.insert(tx_address_1_nonce_0).unwrap();

    let lowest_address_0 = pool.get_lowest_nonce(contract_address!("0x0")).unwrap();
    assert_eq!(lowest_address_0, nonce!(1));
    let lowest_address_1 = pool.get_lowest_nonce(contract_address!("0x1")).unwrap();
    assert_eq!(lowest_address_1, nonce!(0));
    let no_more_txs = pool.get_lowest_nonce(contract_address!("0x2"));
    assert!(no_more_txs.is_none());

    pool.remove(tx_hash!(1)).unwrap();
    let lowest_address_0_after_removal = pool.get_lowest_nonce(contract_address!("0x0")).unwrap();
    assert_eq!(lowest_address_0_after_removal, nonce!(3));
}
