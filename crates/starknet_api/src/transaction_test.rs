use assert_matches::assert_matches;
use rstest::{fixture, rstest};

use super::Transaction;
use crate::block::NonzeroGasPrice;
use crate::core::ChainId;
use crate::executable_transaction::{
    AccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
    Transaction as ExecutableTransaction,
};
use crate::execution_resources::GasAmount;
use crate::test_utils::{read_json_file, TransactionTestData};
use crate::transaction::Fee;

#[fixture]
fn transactions_data() -> Vec<TransactionTestData> {
    // The details were taken from Starknet Mainnet. You can find the transactions by hash in:
    // https://alpha-mainnet.starknet.io/feeder_gateway/get_transaction?transactionHash=<transaction_hash>
    serde_json::from_value(read_json_file("transaction_hash.json")).unwrap()
}

fn verify_transaction_conversion(tx: &Transaction, expected_executable_tx: ExecutableTransaction) {
    let converted_executable_tx: ExecutableTransaction =
        (tx.clone(), &ChainId::Mainnet).try_into().unwrap();
    let reconverted_tx = Transaction::from(converted_executable_tx.clone());

    assert_eq!(converted_executable_tx, expected_executable_tx);
    assert_eq!(tx, &reconverted_tx);
}

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

#[rstest]
fn test_invoke_executable_transaction_conversion(mut transactions_data: Vec<TransactionTestData>) {
    // Extract Invoke transaction data.
    let transaction_data = transactions_data.remove(0);
    let tx = transaction_data.transaction;
    let invoke_tx = assert_matches!(
        tx,
        Transaction::Invoke(ref invoke_tx) => invoke_tx.clone(),
        "Transaction_hash.json is expected to have Invoke as the first transaction."
    );

    let expected_executable_tx =
        ExecutableTransaction::Account(AccountTransaction::Invoke(InvokeTransaction {
            tx: invoke_tx,
            tx_hash: transaction_data.transaction_hash,
        }));

    verify_transaction_conversion(&tx, expected_executable_tx);
}

#[rstest]
fn test_l1_handler_executable_transaction_conversion(
    mut transactions_data: Vec<TransactionTestData>,
) {
    // Extract L1 Handler transaction data.
    let transaction_data = transactions_data.remove(10);
    let tx = transaction_data.transaction;
    let l1_handler_tx = assert_matches!(
        tx,
        Transaction::L1Handler(ref l1_handler_tx) => l1_handler_tx.clone(),
        "Transaction_hash.json is expected to have L1 handler as the 11th transaction."
    );

    let expected_executable_tx = ExecutableTransaction::L1Handler(L1HandlerTransaction {
        tx: l1_handler_tx,
        tx_hash: transaction_data.transaction_hash,
        paid_fee_on_l1: Fee(1),
    });

    verify_transaction_conversion(&tx, expected_executable_tx);
}
