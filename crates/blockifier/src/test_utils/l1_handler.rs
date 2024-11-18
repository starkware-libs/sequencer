use starknet_api::calldata;
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionVersion;
use starknet_types_core::felt::Felt;

use crate::abi::abi_utils::selector_from_name;

pub fn l1handler_tx(l1_fee: Fee, contract_address: ContractAddress) -> L1HandlerTransaction {
    let calldata = calldata![
        Felt::from(0x123), // from_address.
        Felt::from(0x876), // key.
        Felt::from(0x44)   // value.
    ];

    executable_l1_handler_tx(L1HandlerTxArgs {
        version: TransactionVersion::ZERO,
        contract_address,
        entry_point_selector: selector_from_name("l1_handler_set_value"),
        calldata,
        paid_fee_on_l1: l1_fee,
        ..Default::default()
    })
}
