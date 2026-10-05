use starknet_api::block::{BlockFeeMarketInfo, BlockNumber, GasPrice};

use crate::fee_market::{FeeMarketInfoStorageReader, FeeMarketInfoStorageWriter};
use crate::test_utils::get_test_storage;

fn dummy_fee_market_info() -> BlockFeeMarketInfo {
    BlockFeeMarketInfo { fee_proposal_fri: Some(GasPrice(17)), next_l2_gas_price: GasPrice(23) }
}

#[test]
fn set_get_and_revert_fee_market_info() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);
    let fee_market_info = dummy_fee_market_info();

    assert_eq!(reader.begin_ro_txn().unwrap().get_fee_market_info(height).unwrap(), None);

    writer
        .begin_rw_txn()
        .unwrap()
        .set_fee_market_info(height, &fee_market_info)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(
        reader.begin_ro_txn().unwrap().get_fee_market_info(height).unwrap(),
        Some(fee_market_info)
    );
    assert_eq!(reader.begin_ro_txn().unwrap().get_fee_market_info(BlockNumber(6)).unwrap(), None);

    // Overwriting a committed height is rejected.
    assert!(writer.begin_rw_txn().unwrap().set_fee_market_info(height, &fee_market_info).is_err());

    writer.begin_rw_txn().unwrap().revert_fee_market_info(height).unwrap().commit().unwrap();
    assert_eq!(reader.begin_ro_txn().unwrap().get_fee_market_info(height).unwrap(), None);

    // Reverting an absent height is a no-op.
    writer.begin_rw_txn().unwrap().revert_fee_market_info(height).unwrap().commit().unwrap();
}
