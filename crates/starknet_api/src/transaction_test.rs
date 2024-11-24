use super::Transaction;
use crate::block::NonzeroGasPrice;
use crate::executable_transaction::Transaction as ExecutableTransaction;
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
    let mut transactions_test_data_vec: Vec<TransactionTestData> =
        serde_json::from_value(read_json_file("transaction_hash.json")).unwrap();

    let (tx, tx_hash) = loop {
        match transactions_test_data_vec.pop() {
            Some(data) => {
                if let Transaction::Invoke(tx) = data.transaction {
                    // Do something with the data
                    break (Transaction::Invoke(tx), data.transaction_hash);
                }
            }
            None => {
                panic!("Could not find a single Invoke transaction in the test data");
            }
        }
    };
    let executable_tx: ExecutableTransaction = (tx.clone(), tx_hash).into();
    let tx_back = Transaction::from(executable_tx);
    assert_eq!(tx, tx_back);
}
