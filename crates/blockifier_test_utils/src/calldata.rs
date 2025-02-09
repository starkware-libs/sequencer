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
