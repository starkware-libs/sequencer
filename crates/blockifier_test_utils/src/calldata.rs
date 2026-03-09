use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::ContractAddress;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;

use crate::types::u64_from_usize;

/// Creates the calldata for the "__execute__" entry point in the featured contracts
/// ([`crate::contracts::FeatureContract`]) AccountWithLongValidate and AccountWithoutValidations.
/// The format of the returned calldata is:
/// [
///     contract_address,
///     entry_point_name,
///     calldata_length,
///     *calldata,
/// ]
/// The contract_address is the address of the called contract, the entry_point_name is the
/// name of the called entry point in said contract, and the calldata is the calldata for the
/// called entry point.
pub fn create_calldata(
    contract_address: ContractAddress,
    entry_point_name: &str,
    entry_point_args: &[Felt],
) -> Calldata {
    Calldata(
        [
            vec![
                *contract_address.0.key(),              // Contract address.
                selector_from_name(entry_point_name).0, // EP selector name.
                felt!(u64_from_usize(entry_point_args.len())),
            ],
            entry_point_args.into(),
        ]
        .concat()
        .into(),
    )
}

/// Calldata for a trivial entry point in the [`crate::contracts::FeatureContract`] TestContract.
/// The calldata is formatted for using the featured contracts AccountWithLongValidate or
/// AccountWithoutValidations as account contract.
/// The contract_address is the address of the called contract, an instance address of
/// TestContract.
pub fn create_trivial_calldata(test_contract_address: ContractAddress) -> Calldata {
    create_calldata(
        test_contract_address,
        "return_result",
        &[felt!(2_u8)], // Calldata: num.
    )
}

/// Calldata for the reverted inner Cairo0 execution scenario:
/// C1 (A) calls B.
/// C1 (B) calls C.
/// C0 (C) sets key->value, returns to B.
/// C1 (B) panics.
/// C1 (A) catches the panic and completes execution (ignores the error).
pub fn cairo0_proven_revert_scenario_calldata(
    cairo1_contract_address: ContractAddress,
    cairo0_contract_address: ContractAddress,
    key: Felt,
    value: Felt,
) -> Calldata {
    // Contract C (Cairo 0): test_storage_write(address, value).
    let contract_c_calldata = [key, value];

    // Contract B (Cairo 1): middle_revert_contract(contract_address, entry_point_selector,
    // calldata).
    // Calls contract C's test_storage_write, then panics.
    let contract_b_calldata = [
        vec![
            **cairo0_contract_address,
            selector_from_name("test_storage_read_write").0,
            contract_c_calldata.len().into(),
        ],
        contract_c_calldata.to_vec(),
    ]
    .concat();

    // Contract A (Cairo 1): test_call_contract_revert(contract_address, entry_point_selector,
    // calldata, is_meta_tx).
    // Calls contract B's middle_revert_contract and catches the panic.
    let contract_a_calldata = [
        vec![
            **cairo1_contract_address,
            selector_from_name("middle_revert_contract").0,
            contract_b_calldata.len().into(),
        ],
        contract_b_calldata,
        vec![false.into()], // is_meta_tx.
    ]
    .concat();

    create_calldata(cairo1_contract_address, "test_call_contract_revert", &contract_a_calldata)
}
