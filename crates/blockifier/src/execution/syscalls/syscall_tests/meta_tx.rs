use blockifier_test_utils::cairo_versions::RunnableCairo1;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::Felt252;
use starknet_api::abi::abi_utils::{selector_from_name, starknet_keccak};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_api::transaction::{TransactionHash, TransactionVersion, QUERY_VERSION_BASE};
use starknet_types_core::hash::{Pedersen, StarkHash};
use test_case::test_case;

use crate::context::ChainInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::CallEntryPoint;
use crate::state::state_api::StateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{trivial_external_entry_point_with_address, BALANCE};
use crate::transaction::objects::{CommonAccountFields, CurrentTransactionInfo, TransactionInfo};

#[test_case(RunnableCairo1::Casm, ExecutionMode::Execute, false; "VM, execute")]
#[test_case(RunnableCairo1::Casm, ExecutionMode::Execute, true; "VM, execute, only_query")]
#[test_case(RunnableCairo1::Casm, ExecutionMode::Validate, false; "VM, validate")]
// TODO(lior): Uncomment when native supports `meta_tx_v0`.
// #[cfg_attr(
//     feature = "cairo_native",
//     test_case(
//         RunnableCairo1::Native,
//         ExecutionMode::Execute,
//         false;
//         "Native, execute"
//     )
// )]
// #[cfg_attr(
//     feature = "cairo_native",
//     test_case(
//         RunnableCairo1::Native,
//         ExecutionMode::Execute,
//         true;
//         "Native, execute, only_query"
//     )
// )]
// #[cfg_attr(
//     feature = "cairo_native",
//     test_case(
//         RunnableCairo1::Native,
//         ExecutionMode::Validate,
//         false;
//         "Native, validate"
//     )
// )]
fn test_meta_tx_v0(
    runnable_version: RunnableCairo1,
    execution_mode: ExecutionMode,
    only_query: bool,
) {
    let meta_tx_contract = FeatureContract::MetaTx(runnable_version);
    let mut state = test_state(&ChainInfo::create_for_testing(), BALANCE, &[(meta_tx_contract, 1)]);

    // Prepare some constants.
    let contract_address = meta_tx_contract.get_instance_address(0);
    let argument: Felt252 = 1234.into();
    let signature0: Felt252 = 1000.into();
    let signature1: Felt252 = 17.into();
    let nonce: Felt252 = 13.into();
    let tx_hash: Felt252 = 0xabcdef.into();
    let account_address: ContractAddress = 0xfedcba0000_u128.into();
    let expected_version = felt!(3_u32) + (if only_query { *QUERY_VERSION_BASE } else { 0.into() });
    let expected_meta_tx_version = if only_query { *QUERY_VERSION_BASE } else { 0.into() };

    let calldata = Calldata(
        vec![
            contract_address.into(),
            selector_from_name("foo").0,
            // Inner calldata.
            1.into(),
            argument,
            // Inner signature.
            2.into(),
            signature0,
            signature1,
        ]
        .into(),
    );

    let entry_point_call = CallEntryPoint {
        entry_point_selector: selector_from_name("execute_meta_tx_v0"),
        calldata,
        caller_address: account_address,
        ..trivial_external_entry_point_with_address(contract_address)
    };

    let tx_info = TransactionInfo::Current(CurrentTransactionInfo {
        common_fields: CommonAccountFields {
            transaction_hash: TransactionHash(tx_hash),
            version: TransactionVersion::THREE,
            signature: Default::default(),
            nonce: Nonce(nonce),
            sender_address: account_address,
            only_query,
        },
        ..CurrentTransactionInfo::create_for_testing()
    });

    let exec_result =
        entry_point_call.execute_directly_given_tx_info(&mut state, tx_info, false, execution_mode);

    match execution_mode {
        ExecutionMode::Execute => {
            assert!(!exec_result.unwrap().execution.failed);
        }
        ExecutionMode::Validate => {
            assert!(exec_result.is_err());
            return;
        }
    }

    let check_value = |key: Felt252, value: Felt252| {
        assert_eq!(state.get_storage_at(contract_address, key.try_into().unwrap()).unwrap(), value)
    };
    let from_bytes = |bytes| Felt252::from_bytes_be_slice(bytes);

    let call_data_key = starknet_keccak("call_data".as_bytes());
    let call_data_item0_key = Pedersen::hash(&call_data_key, &0.into());
    let call_data_item1_key = Pedersen::hash(&call_data_key, &1.into());

    // Size of `call_data` vector.
    check_value(call_data_key, 2.into());

    // Inside the meta-tx.
    check_value(call_data_item0_key + 0, 0.into()); // caller_address.
    check_value(call_data_item0_key + 1, contract_address.into()); // account_contract_address.
    check_value(call_data_item0_key + 2, expected_meta_tx_version); // tx_version.
    check_value(call_data_item0_key + 3, argument); // argument.
    // TODO(lior): Once meta tx is implemented, replace with meta tx hash.
    check_value(call_data_item0_key + 4, tx_hash); // transaction_hash.
    check_value(call_data_item0_key + 5, signature0); // signature.
    check_value(call_data_item0_key + 6, 0.into()); // max_fee.
    check_value(call_data_item0_key + 7, 0.into()); // resource_bound_len.
    check_value(call_data_item0_key + 8, 0.into()); // nonce.

    // Outside the meta-tx.
    check_value(call_data_item1_key + 0, account_address.into()); // caller_address
    check_value(call_data_item1_key + 1, account_address.into()); // account_contract_address.
    check_value(call_data_item1_key + 2, expected_version); // tx_version.
    check_value(call_data_item1_key + 3, from_bytes(b"NO_ARGUMENT")); // argument.
    check_value(call_data_item1_key + 4, tx_hash); // transaction_hash.
    check_value(call_data_item1_key + 5, from_bytes(b"NO_SIGNATURE")); // signature.
    check_value(call_data_item1_key + 6, 0.into()); // max_fee.
    check_value(call_data_item1_key + 7, 3.into()); // resource_bound_len.
    check_value(call_data_item1_key + 8, nonce); // nonce.
}
