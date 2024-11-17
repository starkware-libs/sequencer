use crate::block::NonzeroGasPrice;
use crate::execution_resources::GasAmount;
use crate::test_utils::{read_json_file, TransactionTestData};
use crate::transaction::Fee;

#[test]
fn test_fee_div_ceil() {
    assert_eq!(
        GasAmount(1),
        Fee(1).checked_div_ceil(NonzeroGasPrice::try_from(1_u8).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(0),
        Fee(0).checked_div_ceil(NonzeroGasPrice::try_from(1_u8).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(1),
        Fee(1).checked_div_ceil(NonzeroGasPrice::try_from(2_u8).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(9),
        Fee(27).checked_div_ceil(NonzeroGasPrice::try_from(3_u8).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(10),
        Fee(28).checked_div_ceil(NonzeroGasPrice::try_from(3_u8).unwrap()).unwrap()
    );
}

#[test]
fn convert_executable_transaction_and_back() {
    // The details were taken from Starknet Mainnet. You can find the transactions by hash in:
    // https://alpha-mainnet.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    let transactions_test_data_vec: Vec<TransactionTestData> =
        serde_json::from_value(read_json_file("transaction_hash.json")).unwrap();

    for transaction_test_data in transactions_test_data_vec {
        print!("{:?}", transaction_test_data);
    }
}
