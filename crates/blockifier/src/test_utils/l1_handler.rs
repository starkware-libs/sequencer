use std::sync::LazyLock;

use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::calldata;
use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::fields::Fee;
use starknet_types_core::felt::Felt;

// This selector is a property of 'FeatureContract::TestContract'.
pub static L1_HANDLER_SET_VALUE_ENTRY_POINT_SELECTOR: LazyLock<EntryPointSelector> =
    LazyLock::new(|| selector_from_name("l1_handler_set_value"));

pub fn l1handler_tx(l1_fee: Fee, contract_address: ContractAddress) -> L1HandlerTransaction {
    let calldata = calldata![
        Felt::from(0x123), // from_address.
        Felt::from(0x876), // key.
        Felt::from(0x44)   // value.
    ];

    executable_l1_handler_tx(L1HandlerTxArgs {
        contract_address,
        entry_point_selector: *L1_HANDLER_SET_VALUE_ENTRY_POINT_SELECTOR,
        calldata,
        paid_fee_on_l1: l1_fee,
        ..Default::default()
    })
}

pub fn l1_handler_set_value_and_revert(
    l1_fee: Fee,
    contract_address: ContractAddress,
) -> L1HandlerTransaction {
    let calldata = calldata![
        Felt::from(0x123), // from_address.
        Felt::from(0x876), // key.
        Felt::from(0x55)   // value.
    ];

    executable_l1_handler_tx(L1HandlerTxArgs {
        contract_address,
        entry_point_selector: selector_from_name("l1_handler_set_value_and_revert"),
        calldata,
        paid_fee_on_l1: l1_fee,
        ..Default::default()
    })
}
