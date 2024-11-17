use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use regex::Regex;
use rstest::rstest;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
};
use starknet_api::executable_transaction::AccountTransaction as Transaction;
use starknet_api::transaction::constants::{
    DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
    EXECUTE_ENTRY_POINT_NAME,
    FELT_TRUE,
    VALIDATE_DECLARE_ENTRY_POINT_NAME,
    VALIDATE_DEPLOY_ENTRY_POINT_NAME,
    VALIDATE_ENTRY_POINT_NAME,
};
use starknet_api::transaction::fields::{
    ContractAddressSalt,
    Fee,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionVersion;
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::abi::abi_utils::selector_from_name;
use crate::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use crate::context::{BlockContext, ChainInfo};
use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::stack_trace::{
    extract_trailing_cairo1_revert_trace,
    Cairo1RevertHeader,
    Cairo1RevertSummary,
    MIN_CAIRO1_FRAME_LENGTH,
    TRACE_LENGTH_CAP,
};
use crate::execution::syscalls::hint_processor::ENTRYPOINT_FAILED_ERROR;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::{fund_account, test_state};
use crate::test_utils::{create_calldata, CairoVersion, BALANCE};
use crate::transaction::test_utils::{
    account_invoke_tx,
    block_context,
    create_account_tx_for_validate_test_nonce_0,
    default_all_resource_bounds,
    run_invoke_tx,
    FaultyAccountTxCreatorArgs,
    INVALID,
};
use crate::transaction::transaction_types::TransactionType;
use crate::transaction::transactions::ExecutableTransaction;

#[rstest]
fn test_stack_trace_with_inner_error_msg(block_context: BlockContext) {
    let cairo_version = CairoVersion::Cairo0;
    let chain_info = ChainInfo::create_for_testing();
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&chain_info, BALANCE, &[(account, 1), (test_contract, 2)]);
    let account_address = account.get_instance_address(0);
    let test_contract_address = test_contract.get_instance_address(0);
    let test_contract_address_2 = test_contract.get_instance_address(1);
    let account_address_felt = *account_address.0.key();
    let test_contract_address_felt = *test_contract_address.0.key();
    let test_contract_address_2_felt = *test_contract_address_2.0.key();
    let test_contract_hash = test_contract.get_class_hash().0;
    let account_contract_hash = account.get_class_hash().0;

    // Nest calls: __execute__ -> test_call_contract_fail_with_attr_error_msg -> assert_0_is_1.
    let call_contract_function_name = "test_call_contract_fail_with_attr_error_msg";
    let inner_entry_point_selector_felt = selector_from_name("fail").0;
    let calldata = create_calldata(
        test_contract_address, // contract_address
        call_contract_function_name,
        &[
            test_contract_address_2_felt,    // Contract address.
            inner_entry_point_selector_felt, // Function selector.
        ],
    );

    let tx_execution_error = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            sender_address: account_address,
            calldata,
            version: TransactionVersion::ZERO,
        },
    )
    .unwrap_err();

    // Fetch PC locations from the compiled contract to compute the expected PC locations in the
    // traceback. Computation is not robust, but as long as the cairo function itself is not edited,
    // this computation should be stable.
    let account_entry_point_offset =
        account.get_entry_point_offset(selector_from_name(EXECUTE_ENTRY_POINT_NAME));
    let execute_selector_felt = selector_from_name(EXECUTE_ENTRY_POINT_NAME).0;
    let external_entry_point_selector_felt = selector_from_name(call_contract_function_name).0;
    let entry_point_offset =
        test_contract.get_entry_point_offset(selector_from_name(call_contract_function_name));
    // Relative offsets of the test_call_contract entry point and the inner call.
    let call_location = entry_point_offset.0 + 6;
    let entry_point_location = entry_point_offset.0 - 4;
    // Relative offsets of the account contract.
    let account_call_location = account_entry_point_offset.0 + 18;
    let account_entry_point_location = account_entry_point_offset.0 - 8;

    let expected_trace = format!(
        "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
         {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error at pc=0:7:
Cairo traceback (most recent call last):
Unknown location (pc=0:{account_call_location})
Unknown location (pc=0:{account_entry_point_location})

1: Error in the called contract (contract address: {test_contract_address_felt:#064x}, class hash: \
         {test_contract_hash:#064x}, selector: {external_entry_point_selector_felt:#064x}):
Error at pc=0:37:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Error message: Be aware of failure ahead...
Unknown location (pc=0:{entry_point_location})

2: Error in the called contract (contract address: {test_contract_address_2_felt:#064x}, class \
         hash: {test_contract_hash:#064x}, selector: {inner_entry_point_selector_felt:#064x}):
Error message: You shall not pass!
Error at pc=0:1294:
Cairo traceback (most recent call last):
Unknown location (pc=0:1298)

An ASSERT_EQ instruction failed: 1 != 0.
"
    );

    assert_eq!(tx_execution_error.to_string(), expected_trace);
}

#[rstest]
fn test_stack_trace(
    block_context: BlockContext,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let chain_info = ChainInfo::create_for_testing();
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&chain_info, BALANCE, &[(account, 1), (test_contract, 2)]);
    let account_address = account.get_instance_address(0);
    let test_contract_address = test_contract.get_instance_address(0);
    let test_contract_address_2 = test_contract.get_instance_address(1);
    let account_address_felt = *account_address.0.key();
    let test_contract_address_felt = *test_contract_address.0.key();
    let test_contract_address_2_felt = *test_contract_address_2.0.key();
    let test_contract_hash = test_contract.get_class_hash().0;
    let account_contract_hash = account.get_class_hash().0;

    // Nest calls: __execute__ -> test_call_contract -> assert_0_is_1.
    let call_contract_function_name = "test_call_contract";
    let inner_entry_point_selector_felt = selector_from_name("fail").0;
    let calldata = create_calldata(
        test_contract_address, // contract_address
        call_contract_function_name,
        &[
            test_contract_address_2_felt,    // Contract address.
            inner_entry_point_selector_felt, // Function selector.
            felt!(0_u8),                     // Innermost calldata length.
        ],
    );

    let tx_execution_error = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            sender_address: account_address,
            calldata,
            version: TransactionVersion::ZERO,
        },
    )
    .unwrap_err();

    // Fetch PC locations from the compiled contract to compute the expected PC locations in the
    // traceback. Computation is not robust, but as long as the cairo function itself is not edited,
    // this computation should be stable.
    let account_entry_point_offset =
        account.get_entry_point_offset(selector_from_name(EXECUTE_ENTRY_POINT_NAME));
    let execute_selector_felt = selector_from_name(EXECUTE_ENTRY_POINT_NAME).0;
    let external_entry_point_selector_felt = selector_from_name(call_contract_function_name).0;
    let entry_point_offset =
        test_contract.get_entry_point_offset(selector_from_name(call_contract_function_name));
    // Relative offsets of the test_call_contract entry point and the inner call.
    let call_location = entry_point_offset.0 + 14;
    let entry_point_location = entry_point_offset.0 - 3;
    // Relative offsets of the account contract.
    let account_call_location = account_entry_point_offset.0 + 18;
    let account_entry_point_location = account_entry_point_offset.0 - 8;

    let expected_trace_cairo0 = format!(
        "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
         {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error at pc=0:7:
Cairo traceback (most recent call last):
Unknown location (pc=0:{account_call_location})
Unknown location (pc=0:{account_entry_point_location})

1: Error in the called contract (contract address: {test_contract_address_felt:#064x}, class hash: \
         {test_contract_hash:#064x}, selector: {external_entry_point_selector_felt:#064x}):
Error at pc=0:37:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Unknown location (pc=0:{entry_point_location})

2: Error in the called contract (contract address: {test_contract_address_2_felt:#064x}, class \
         hash: {test_contract_hash:#064x}, selector: {inner_entry_point_selector_felt:#064x}):
Error message: You shall not pass!
Error at pc=0:1294:
Cairo traceback (most recent call last):
Unknown location (pc=0:1298)

An ASSERT_EQ instruction failed: 1 != 0.
"
    );

    let expected_trace_cairo1 = format!(
        "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
         {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Execution failed. Failure reason:
Error in contract (contract address: {account_address_felt:#064x}, class hash: \
         {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error in contract (contract address: {test_contract_address_felt:#064x}, class hash: \
         {test_contract_hash:#064x}, selector: {external_entry_point_selector_felt:#064x}):
Error in contract (contract address: {test_contract_address_2_felt:#064x}, class hash: \
         {test_contract_hash:#064x}, selector: {inner_entry_point_selector_felt:#064x}):
0x6661696c ('fail').
"
    );

    let expected_trace = match cairo_version {
        CairoVersion::Cairo0 => expected_trace_cairo0,
        CairoVersion::Cairo1 => expected_trace_cairo1,
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => panic!("Cairo Native is not yet supported"),
    };

    assert_eq!(tx_execution_error.to_string(), expected_trace);
}

#[rstest]
#[case(CairoVersion::Cairo0, "invoke_call_chain", "Couldn't compute operand op0. Unknown value for memory cell 1:37", (1191_u16, 1237_u16))]
#[case(CairoVersion::Cairo0, "fail", "An ASSERT_EQ instruction failed: 1 != 0.", (1294_u16, 1245_u16))]
#[case(CairoVersion::Cairo1, "invoke_call_chain", "0x4469766973696f6e2062792030 ('Division by 0')", (0_u16, 0_u16))]
#[case(CairoVersion::Cairo1, "fail", "0x6661696c ('fail')", (0_u16, 0_u16))]
fn test_trace_callchain_ends_with_regular_call(
    block_context: BlockContext,
    #[case] cairo_version: CairoVersion,
    #[case] last_func_name: &str,
    #[case] expected_error: &str,
    #[case] expected_pc_locations: (u16, u16),
) {
    let expected_with_attr_error_msg = match (cairo_version, last_func_name) {
        (CairoVersion::Cairo0, "fail") => "Error message: You shall not pass!\n".to_string(),
        _ => String::new(),
    };
    let chain_info = ChainInfo::create_for_testing();
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);

    let account_address = account_contract.get_instance_address(0);
    let test_contract_address = test_contract.get_instance_address(0);
    let account_address_felt = *account_address.0.key();
    let contract_address_felt = *test_contract_address.0.key();
    let test_contract_hash = test_contract.get_class_hash().0;
    let account_contract_hash = account_contract.get_class_hash().0;

    // invoke_call_chain -> call_contract_syscall invoke_call_chain -> regular call to final func.
    let invoke_call_chain_selector = selector_from_name("invoke_call_chain");
    let invoke_call_chain_selector_felt = invoke_call_chain_selector.0;

    let calldata = create_calldata(
        test_contract_address, // contract_address
        "invoke_call_chain",
        &[
            felt!(7_u8),                          // Calldata length
            contract_address_felt,                // Contract address.
            invoke_call_chain_selector_felt,      // Function selector.
            felt!(0_u8),                          // Call type: call_contract_syscall.
            felt!(3_u8),                          // Calldata length
            contract_address_felt,                // Contract address.
            selector_from_name(last_func_name).0, // Function selector.
            felt!(2_u8),                          // Call type: regular call.
        ],
    );

    let tx_execution_error = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            sender_address: account_address,
            calldata,
            version: TransactionVersion::ZERO,
        },
    )
    .unwrap_err();

    let account_entry_point_offset =
        account_contract.get_entry_point_offset(selector_from_name(EXECUTE_ENTRY_POINT_NAME));
    let entry_point_offset = test_contract.get_entry_point_offset(invoke_call_chain_selector);
    let execute_selector_felt = selector_from_name(EXECUTE_ENTRY_POINT_NAME).0;

    let expected_trace = match cairo_version {
        CairoVersion::Cairo0 => {
            let call_location = entry_point_offset.0 + 12;
            let entry_point_location = entry_point_offset.0 - 61;
            // Relative offsets of the account contract.
            let account_call_location = account_entry_point_offset.0 + 18;
            let account_entry_point_location = account_entry_point_offset.0 - 8;
            // Final invocation locations.
            let (expected_pc0, expected_pc1) = expected_pc_locations;
            format!(
                "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error at pc=0:7:
Cairo traceback (most recent call last):
Unknown location (pc=0:{account_call_location})
Unknown location (pc=0:{account_entry_point_location})

1: Error in the called contract (contract address: {contract_address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
Error at pc=0:37:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Unknown location (pc=0:{entry_point_location})

2: Error in the called contract (contract address: {contract_address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
{expected_with_attr_error_msg}Error at pc=0:{expected_pc0}:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Unknown location (pc=0:{expected_pc1})

{expected_error}
"
            )
        }
        CairoVersion::Cairo1 => {
            format!(
                "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Execution failed. Failure reason:
Error in contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error in contract (contract address: {contract_address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
Error in contract (contract address: {contract_address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
{expected_error}.
"
            )
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => {
            todo!("Cairo Native is not yet supported here")
        }
    };

    assert_eq!(tx_execution_error.to_string(), expected_trace);
}

#[rstest]
#[case(CairoVersion::Cairo0, "invoke_call_chain", "Couldn't compute operand op0. Unknown value for memory cell 1:23", 1_u8, 0_u8, (37_u16, 1203_u16, 1191_u16, 1276_u16))]
#[case(CairoVersion::Cairo0, "invoke_call_chain", "Couldn't compute operand op0. Unknown value for memory cell 1:23", 1_u8, 1_u8, (49_u16, 1221_u16, 1191_u16, 1276_u16))]
#[case(CairoVersion::Cairo0, "fail", "An ASSERT_EQ instruction failed: 1 != 0.", 0_u8, 0_u8, (37_u16, 1203_u16, 1294_u16, 1298_u16))]
#[case(CairoVersion::Cairo0, "fail", "An ASSERT_EQ instruction failed: 1 != 0.", 0_u8, 1_u8, (49_u16, 1221_u16, 1294_u16, 1298_u16))]
#[case(CairoVersion::Cairo1, "invoke_call_chain", "0x4469766973696f6e2062792030 ('Division by 0')", 1_u8, 0_u8, (9631_u16, 9631_u16, 0_u16, 0_u16))]
#[case(CairoVersion::Cairo1, "invoke_call_chain", "0x4469766973696f6e2062792030 ('Division by 0')", 1_u8, 1_u8, (9631_u16, 9700_u16, 0_u16, 0_u16))]
#[case(CairoVersion::Cairo1, "fail", "0x6661696c ('fail')", 0_u8, 0_u8, (9631_u16, 9631_u16, 0_u16, 0_u16))]
#[case(CairoVersion::Cairo1, "fail", "0x6661696c ('fail')", 0_u8, 1_u8, (9631_u16, 9700_u16, 0_u16, 0_u16))]
fn test_trace_call_chain_with_syscalls(
    block_context: BlockContext,
    #[case] cairo_version: CairoVersion,
    #[case] last_func_name: &str,
    #[case] expected_error: &str,
    #[case] calldata_extra_length: u8,
    #[case] call_type: u8,
    #[case] expected_pcs: (u16, u16, u16, u16),
) {
    let expected_with_attr_error_msg = match (cairo_version, last_func_name) {
        (CairoVersion::Cairo0, "fail") => "Error message: You shall not pass!\n".to_string(),
        _ => String::new(),
    };
    let chain_info = ChainInfo::create_for_testing();
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let mut state = test_state(&chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);

    let account_address = account_contract.get_instance_address(0);
    let test_contract_address = test_contract.get_instance_address(0);

    let test_contract_hash = test_contract.get_class_hash().0;
    let account_contract_hash = account_contract.get_class_hash().0;
    let account_address_felt = *account_address.0.key();
    let address_felt = *test_contract_address.0.key();
    let contract_id = if call_type == 0 { address_felt } else { test_contract_hash };

    // invoke_call_chain -> call_contract_syscall invoke_call_chain -> call_contract_syscall /
    // library_call_syscall to final func.
    let invoke_call_chain_selector = selector_from_name("invoke_call_chain");
    let invoke_call_chain_selector_felt = invoke_call_chain_selector.0;
    let last_func_selector_felt = selector_from_name(last_func_name).0;

    let mut raw_calldata = vec![
        felt!(7_u8 + calldata_extra_length), // Calldata length
        address_felt,                        // Contract address.
        invoke_call_chain_selector_felt,     // Function selector.
        felt!(0_u8),                         // Call type: call_contract_syscall.
        felt!(3_u8 + calldata_extra_length), // Calldata length
        contract_id,                         // Contract address / class hash.
        last_func_selector_felt,             // Function selector.
        felt!(call_type),                    // Syscall type: library_call or call_contract.
    ];

    // Need to send an empty array for the last call in `invoke_call_chain` variant.
    if last_func_name == "invoke_call_chain" {
        raw_calldata.push(felt!(0_u8));
    }

    let calldata = create_calldata(
        test_contract_address, // contract_address
        "invoke_call_chain",
        &raw_calldata,
    );

    let tx_execution_error = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            sender_address: account_address,
            calldata,
            version: TransactionVersion::ZERO,
        },
    )
    .unwrap_err();

    let account_entry_point_offset =
        account_contract.get_entry_point_offset(selector_from_name(EXECUTE_ENTRY_POINT_NAME));
    let entry_point_offset = test_contract.get_entry_point_offset(invoke_call_chain_selector);
    let execute_selector_felt = selector_from_name(EXECUTE_ENTRY_POINT_NAME).0;

    let last_call_preamble = if call_type == 0 {
        format!(
            "Error in the called contract (contract address: {address_felt:#064x}, class hash: \
             {test_contract_hash:#064x}, selector: {last_func_selector_felt:#064x})"
        )
    } else {
        format!(
            "Error in a library call (contract address: {address_felt:#064x}, class hash: \
             {test_contract_hash:#064x}, selector: {last_func_selector_felt:#064x})"
        )
    };

    let expected_trace = match cairo_version {
        CairoVersion::Cairo0 => {
            let call_location = entry_point_offset.0 + 12;
            let entry_point_location = entry_point_offset.0 - 61;
            // Relative offsets of the account contract.
            let account_call_location = account_entry_point_offset.0 + 18;
            let account_entry_point_location = account_entry_point_offset.0 - 8;
            let (expected_pc0, expected_pc1, expected_pc2, expected_pc3) = expected_pcs;
            format!(
                "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error at pc=0:7:
Cairo traceback (most recent call last):
Unknown location (pc=0:{account_call_location})
Unknown location (pc=0:{account_entry_point_location})

1: Error in the called contract (contract address: {address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
Error at pc=0:37:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Unknown location (pc=0:{entry_point_location})

2: Error in the called contract (contract address: {address_felt:#064x}, class hash: \
                 {test_contract_hash:#064x}, selector: {invoke_call_chain_selector_felt:#064x}):
Error at pc=0:{expected_pc0}:
Cairo traceback (most recent call last):
Unknown location (pc=0:{call_location})
Unknown location (pc=0:{expected_pc1})

3: {last_call_preamble}:
{expected_with_attr_error_msg}Error at pc=0:{expected_pc2}:
Cairo traceback (most recent call last):
Unknown location (pc=0:{expected_pc3})

{expected_error}
"
            )
        }
        CairoVersion::Cairo1 => {
            format!(
                "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Execution failed. Failure reason:
Error in contract (contract address: {account_address_felt:#064x}, class hash: \
                 {account_contract_hash:#064x}, selector: {execute_selector_felt:#064x}):
Error in contract (contract address: {address_felt:#064x}, class hash: {test_contract_hash:#064x}, \
                 selector: {invoke_call_chain_selector_felt:#064x}):
Error in contract (contract address: {address_felt:#064x}, class hash: {test_contract_hash:#064x}, \
                 selector: {invoke_call_chain_selector_felt:#064x}):
Error in contract (contract address: {address_felt:#064x}, class hash: {test_contract_hash:#064x}, \
                 selector: {last_func_selector_felt:#064x}):
{expected_error}.
"
            )
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => {
            todo!("Cairo Native not yet supported here.")
        }
    };

    assert_eq!(tx_execution_error.to_string(), expected_trace);
}

// TODO(Arni, 1/5/2024): Cover version 0 declare transaction.
// TODO(Arni, 1/5/2024): Consider version 0 invoke.
#[rstest]
#[case::validate_version_1(
    TransactionType::InvokeFunction,
    VALIDATE_ENTRY_POINT_NAME,
    TransactionVersion::ONE
)]
#[case::validate_version_3(
    TransactionType::InvokeFunction,
    VALIDATE_ENTRY_POINT_NAME,
    TransactionVersion::THREE
)]
#[case::validate_declare_version_1(
    TransactionType::Declare,
    VALIDATE_DECLARE_ENTRY_POINT_NAME,
    TransactionVersion::ONE
)]
#[case::validate_declare_version_2(
    TransactionType::Declare,
    VALIDATE_DECLARE_ENTRY_POINT_NAME,
    TransactionVersion::TWO
)]
#[case::validate_declare_version_3(
    TransactionType::Declare,
    VALIDATE_DECLARE_ENTRY_POINT_NAME,
    TransactionVersion::THREE
)]
#[case::validate_deploy_version_1(
    TransactionType::DeployAccount,
    VALIDATE_DEPLOY_ENTRY_POINT_NAME,
    TransactionVersion::ONE
)]
#[case::validate_deploy_version_3(
    TransactionType::DeployAccount,
    VALIDATE_DEPLOY_ENTRY_POINT_NAME,
    TransactionVersion::THREE
)]
fn test_validate_trace(
    #[case] tx_type: TransactionType,
    #[case] entry_point_name: &str,
    #[case] tx_version: TransactionVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let create_for_account_testing = &BlockContext::create_for_account_testing();
    let block_context = create_for_account_testing;
    let faulty_account = FeatureContract::FaultyAccount(cairo_version);
    let mut sender_address = faulty_account.get_instance_address(0);
    let class_hash = faulty_account.get_class_hash();
    let state = &mut test_state(&block_context.chain_info, Fee(0), &[(faulty_account, 1)]);
    let selector = selector_from_name(entry_point_name).0;

    // Logic failure.
    let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
        scenario: INVALID,
        tx_type,
        tx_version,
        sender_address,
        class_hash,
        ..Default::default()
    });

    if let TransactionType::DeployAccount = tx_type {
        // Deploy account uses the actual address as the sender address.
        match &account_tx.tx {
            Transaction::DeployAccount(tx) => {
                sender_address = tx.contract_address();
            }
            _ => panic!("Expected DeployAccountTransaction type"),
        }
    }

    let contract_address = *sender_address.0.key();

    let expected_error = match cairo_version {
        CairoVersion::Cairo0 => format!(
            "Transaction validation has failed:
0: Error in the called contract (contract address: {contract_address:#064x}, class hash: {:#064x}, \
             selector: {selector:#064x}):
Error at pc=0:0:
Cairo traceback (most recent call last):
Unknown location (pc=0:0)
Unknown location (pc=0:0)

An ASSERT_EQ instruction failed: 1 != 0.
",
            class_hash.0
        ),
        CairoVersion::Cairo1 => format!(
            "The `validate` entry point panicked with:
Error in contract (contract address: {contract_address:#064x}, class hash: {:#064x}, selector: \
             {selector:#064x}):
0x496e76616c6964207363656e6172696f ('Invalid scenario').
",
            class_hash.0
        ),
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => todo!("Cairo Native is not yet supported here."),
    };

    // Clean pc locations from the trace.
    let re = Regex::new(r"pc=0:[0-9]+").unwrap();
    let cleaned_expected_error = &re.replace_all(&expected_error, "pc=0:*");
    let charge_fee = account_tx.enforce_fee();
    let actual_error = account_tx.execute(state, block_context, charge_fee, true).unwrap_err();
    let actual_error_str = actual_error.to_string();
    let cleaned_actual_error = &re.replace_all(&actual_error_str, "pc=0:*");
    // Compare actual trace to the expected trace (sans pc locations).
    assert_eq!(cleaned_actual_error.to_string(), cleaned_expected_error.to_string());
}

#[rstest]
/// Tests that hitting an execution error in an account contract constructor outputs the correct
/// traceback (including correct class hash, contract address and constructor entry point selector).
fn test_account_ctor_frame_stack_trace(
    block_context: BlockContext,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let chain_info = &block_context.chain_info;
    let faulty_account = FeatureContract::FaultyAccount(cairo_version);
    let state = &mut test_state(chain_info, BALANCE, &[(faulty_account, 0)]);
    let class_hash = faulty_account.get_class_hash();

    // Create and execute deploy account transaction that passes validation and fails in the ctor.
    let deploy_account_tx =
        create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
            tx_type: TransactionType::DeployAccount,
            scenario: INVALID,
            class_hash,
            max_fee: BALANCE,
            resource_bounds: default_all_resource_bounds(),
            validate_constructor: true,
            ..Default::default()
        });

    // Fund the account so it can afford the deployment.
    let deploy_address = match &deploy_account_tx.tx {
        Transaction::DeployAccount(deploy_tx) => deploy_tx.contract_address(),
        _ => unreachable!("deploy_account_tx is a DeployAccount"),
    };
    fund_account(chain_info, deploy_address, Fee(BALANCE.0 * 2), &mut state.state);

    let expected_selector = selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME).0;
    let expected_address = deploy_address.0.key();
    let expected_error = format!(
        "Contract constructor execution has failed:
0: Error in the contract class constructor (contract address: {expected_address:#064x}, class \
         hash: {:#064x}, selector: {expected_selector:#064x}):
",
        class_hash.0
    )
    .to_string()
        + &match cairo_version {
            CairoVersion::Cairo0 => "Error at pc=0:223:
Cairo traceback (most recent call last):
Unknown location (pc=0:195)
Unknown location (pc=0:179)

An ASSERT_EQ instruction failed: 1 != 0.
"
            .to_string(),
            CairoVersion::Cairo1 => format!(
                "Execution failed. Failure reason:
Error in contract (contract address: {expected_address:#064x}, class hash: {:#064x}, selector: \
                 {expected_selector:#064x}):
0x496e76616c6964207363656e6172696f ('Invalid scenario').
",
                class_hash.0
            )
            .to_string(),
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => {
                todo!("Cairo Native not yet supported here.")
            }
        };

    // Compare expected and actual error.
    let error = deploy_account_tx.execute(state, &block_context, true, true).unwrap_err();
    assert_eq!(error.to_string(), expected_error);
}

#[rstest]
/// Tests that hitting an execution error in a contract constructor during a deploy syscall outputs
/// the correct traceback (including correct class hash, contract address and constructor entry
/// point selector).
fn test_contract_ctor_frame_stack_trace(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let chain_info = &block_context.chain_info;
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let faulty_ctor = FeatureContract::FaultyAccount(cairo_version);
    // Declare both classes, but only "deploy" the dummy account.
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1), (faulty_ctor, 0)]);
    let account_address = account.get_instance_address(0);
    let account_class_hash = account.get_class_hash();
    let faulty_class_hash = faulty_ctor.get_class_hash();

    let salt = felt!(7_u8);
    // Constructor arg: set to true to fail deployment.
    let validate_constructor = felt!(FELT_TRUE);
    let signature = TransactionSignature(vec![felt!(INVALID)]);
    let expected_deployed_address = calculate_contract_address(
        ContractAddressSalt(salt),
        faulty_class_hash,
        &calldata![validate_constructor],
        account_address,
    )
    .unwrap();
    // Invoke the deploy_contract function on the dummy account to deploy the faulty contract.
    let invoke_deploy_tx = account_invoke_tx(invoke_tx_args! {
        sender_address: account_address,
        signature,
        calldata: create_calldata(
            account_address,
            DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME,
            &[
                faulty_class_hash.0,
                salt,
                felt!(1_u8), // Calldata: ctor args length.
                validate_constructor,
            ]
        ),
        resource_bounds: default_all_resource_bounds,
        nonce: Nonce(felt!(0_u8)),
    });

    // Construct expected output.
    let execute_selector = selector_from_name(EXECUTE_ENTRY_POINT_NAME);
    let deploy_contract_selector = selector_from_name(DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME);
    let ctor_selector = selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME);
    let account_address_felt = *account_address.0.key();
    let faulty_class_hash = faulty_ctor.get_class_hash();
    let expected_address = expected_deployed_address.0.key();

    let (frame_0, frame_1, frame_2) = (
        format!(
            "Transaction execution has failed:
0: Error in the called contract (contract address: {account_address_felt:#064x}, class hash: \
             {:#064x}, selector: {:#064x}):",
            account_class_hash.0, execute_selector.0
        ),
        format!(
            "1: Error in the called contract (contract address: {account_address_felt:#064x}, \
             class hash: {:#064x}, selector: {:#064x}):",
            account_class_hash.0, deploy_contract_selector.0
        ),
        format!(
            "2: Error in the contract class constructor (contract address: \
             {expected_address:#064x}, class hash: {:#064x}, selector: {:#064x}):",
            faulty_class_hash.0, ctor_selector.0
        ),
    );
    let (execute_offset, deploy_offset, ctor_offset) = (
        account.get_entry_point_offset(execute_selector).0,
        account.get_entry_point_offset(deploy_contract_selector).0,
        faulty_ctor.get_ctor_offset(Some(ctor_selector)).0,
    );

    let expected_error = match cairo_version {
        CairoVersion::Cairo0 => {
            format!(
                "{frame_0}
Error at pc=0:7:
Cairo traceback (most recent call last):
Unknown location (pc=0:{})
Unknown location (pc=0:{})

{frame_1}
Error at pc=0:20:
Cairo traceback (most recent call last):
Unknown location (pc=0:{})
Unknown location (pc=0:{})

{frame_2}
Error at pc=0:223:
Cairo traceback (most recent call last):
Unknown location (pc=0:{})
Unknown location (pc=0:{})

An ASSERT_EQ instruction failed: 1 != 0.
",
                execute_offset + 18,
                execute_offset - 8,
                deploy_offset + 14,
                deploy_offset - 12,
                ctor_offset + 7,
                ctor_offset - 9
            )
        }
        CairoVersion::Cairo1 => {
            // TODO(Dori, 1/1/2025): Get lowest level PC locations from Cairo1 errors (ctor offset
            //   does not appear in the trace).
            format!(
                "{frame_0}
Error at pc=0:{}:
{frame_1}
Error at pc=0:{}:
{frame_2}
Execution failed. Failure reason:
Error in contract (contract address: {expected_address:#064x}, class hash: {:#064x}, selector: \
                 {:#064x}):
0x496e76616c6964207363656e6172696f ('Invalid scenario').
",
                execute_offset + 165,
                deploy_offset + 154,
                faulty_class_hash.0,
                ctor_selector.0
            )
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => {
            todo!("Cairo Native not yet supported here.")
        }
    };

    // Compare expected and actual error.
    let error =
        invoke_deploy_tx.execute(state, &block_context, true, true).unwrap().revert_error.unwrap();
    assert_eq!(error.to_string(), expected_error);
}

#[test]
fn test_min_cairo1_frame_length() {
    let failure_hex = "0xdeadbeef";
    let call_info_1_frame = CallInfo {
        call: CallEntryPoint {
            class_hash: Some(ClassHash::default()),
            storage_address: ContractAddress::default(),
            entry_point_selector: EntryPointSelector::default(),
            ..Default::default()
        },
        execution: CallExecution {
            retdata: Retdata(vec![felt!(failure_hex)]),
            failed: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info_2_frames = CallInfo {
        call: CallEntryPoint {
            class_hash: Some(ClassHash::default()),
            storage_address: ContractAddress::default(),
            entry_point_selector: EntryPointSelector::default(),
            ..Default::default()
        },
        execution: CallExecution {
            retdata: Retdata(vec![felt!(failure_hex), felt!(ENTRYPOINT_FAILED_ERROR)]),
            failed: true,
            ..Default::default()
        },
        inner_calls: vec![call_info_1_frame.clone()],
        ..Default::default()
    };
    let error_stack_1_frame =
        extract_trailing_cairo1_revert_trace(&call_info_1_frame, Cairo1RevertHeader::Execution);
    let error_stack_2_frames =
        extract_trailing_cairo1_revert_trace(&call_info_2_frames, Cairo1RevertHeader::Execution);
    let diff = error_stack_2_frames.to_string().len() - error_stack_1_frame.to_string().len();
    assert_eq!(diff, *MIN_CAIRO1_FRAME_LENGTH);
}

#[rstest]
#[case::too_many_frames(TRACE_LENGTH_CAP / *MIN_CAIRO1_FRAME_LENGTH + 10, 1, "too_many_frames")]
// Each (large) felt should require at least 30 chars.
#[case::too_much_retdata(1, TRACE_LENGTH_CAP / 30, "too_much_retdata")]
#[case::both_too_much(
    TRACE_LENGTH_CAP / (2 * *MIN_CAIRO1_FRAME_LENGTH), TRACE_LENGTH_CAP / 40, "both_too_much"
)]
fn test_cairo1_revert_error_truncation(
    #[case] n_frames: usize,
    #[case] n_retdata: usize,
    #[case] scenario: &str,
) {
    let failure_felt = "0xbeefbeefbeefbeefbeefbeefbeefbeef";
    let call = CallEntryPoint {
        class_hash: Some(ClassHash::default()),
        storage_address: ContractAddress::default(),
        entry_point_selector: EntryPointSelector::default(),
        ..Default::default()
    };
    let mut retdata = Retdata(vec![felt!(failure_felt); n_retdata]);
    let mut next_call_info = CallInfo {
        call: call.clone(),
        execution: CallExecution { retdata: retdata.clone(), failed: true, ..Default::default() },
        ..Default::default()
    };
    for _ in 1..n_frames {
        retdata.0.push(felt!(ENTRYPOINT_FAILED_ERROR));
        next_call_info = CallInfo {
            call: call.clone(),
            inner_calls: vec![next_call_info],
            execution: CallExecution {
                retdata: retdata.clone(),
                failed: true,
                ..Default::default()
            },
            ..Default::default()
        };
    }

    // Check that the error message is structured as expected.
    let header_type = Cairo1RevertHeader::Execution;
    let header_str = header_type.to_string();
    let tail_str = ".\n";
    let error_stack = extract_trailing_cairo1_revert_trace(&next_call_info, header_type);
    let error_string = error_stack.to_string();
    let first_frame = error_stack.stack.first().unwrap().to_string();
    let last_frame = error_stack.stack.last().unwrap().to_string();
    let (expected_head, expected_tail) = match scenario {
        // Frames truncated, entire failure reason (a single felt) is output.
        "too_many_frames" => {
            (
                format!("{header_str}\n{first_frame}\n"),
                [
                    Cairo1RevertSummary::TRUNCATION_SEPARATOR.into(),
                    last_frame,
                    // One failure felt.
                    format!("{failure_felt}{tail_str}"),
                ]
                .join("\n"),
            )
        }
        // A single frame, but failure reason itself is too long. No frames printed.
        "too_much_retdata" => (
            format!("{header_str}\n({failure_felt}"),
            Cairo1RevertSummary::TRUNCATION_SEPARATOR.into(),
        ),
        // Too many frames and too much retdata - retdata takes precedence.
        "both_too_much" => {
            let retdata_tail = format!(
                "({}{failure_felt}){tail_str}",
                format!("{failure_felt}, ").repeat(n_retdata - 1)
            );
            (
                format!("{header_str}\n{first_frame}\n"),
                [Cairo1RevertSummary::TRUNCATION_SEPARATOR.into(), last_frame, retdata_tail]
                    .join("\n"),
            )
        }
        _ => panic!("Test not implemented for {n_frames} frames."),
    };
    assert!(error_string.len() <= TRACE_LENGTH_CAP);
    assert_eq!(error_string[..expected_head.len()], expected_head);
    assert_eq!(error_string[error_string.len() - expected_tail.len()..], expected_tail);
}

#[test]
fn test_cairo1_stack_extraction_inner_call_successful() {
    let failure_reason_str = "0x1";
    let error_data = Retdata(vec![felt!(failure_reason_str)]);
    let callinfo = CallInfo {
        execution: CallExecution { retdata: error_data, failed: true, ..Default::default() },
        inner_calls: vec![CallInfo::default()],
        ..Default::default()
    };
    let error = EntryPointExecutionError::ExecutionFailed {
        error_trace: extract_trailing_cairo1_revert_trace(&callinfo, Cairo1RevertHeader::Execution),
    };
    assert_eq!(
        error.to_string(),
        format!(
            "Execution failed. Failure reason:
Error in contract (contract address: {:#064x}, class hash: _, selector: {:#064x}):
{failure_reason_str}.
",
            ContractAddress::default().0.key(),
            EntryPointSelector::default().0
        )
    );
}

#[test]
fn test_ambiguous_inner_cairo1_failure() {
    let (failure_reason_0, failure_reason_1) = (Felt::ONE, Felt::TWO);
    let outer_retdata =
        Retdata(vec![failure_reason_0, failure_reason_1, felt!(ENTRYPOINT_FAILED_ERROR)]);
    let inner_call_info = CallInfo {
        execution: CallExecution {
            retdata: Retdata(vec![failure_reason_0, failure_reason_1]),
            failed: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info = CallInfo {
        execution: CallExecution {
            retdata: outer_retdata.clone(),
            failed: true,
            ..Default::default()
        },
        // Both of these inner calls can be the source of error; expect fallback value.
        inner_calls: vec![inner_call_info.clone(), inner_call_info],
        ..Default::default()
    };
    assert_matches!(
        extract_trailing_cairo1_revert_trace(&call_info, Cairo1RevertHeader::Execution),
        Cairo1RevertSummary { stack, last_retdata, .. }
        if stack.is_empty() && last_retdata == outer_retdata
    );
}

#[rstest]
fn test_inner_cairo1_failure_not_last(#[values(true, false)] last_is_failed: bool) {
    let (failure_reason_0, failure_reason_1) = (Felt::ONE, Felt::TWO);
    let outer_retdata =
        Retdata(vec![failure_reason_0, failure_reason_1, felt!(ENTRYPOINT_FAILED_ERROR)]);
    let first_inner_retdata = Retdata(outer_retdata.0[..outer_retdata.0.len() - 1].into());
    let first_inner_call_info = CallInfo {
        execution: CallExecution {
            retdata: first_inner_retdata.clone(),
            failed: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let last_inner_call_info = CallInfo {
        execution: CallExecution {
            retdata: Retdata(
                // Not a prefix of the outer retdata. Should not be selected as inner failure.
                vec![failure_reason_1, felt!(ENTRYPOINT_FAILED_ERROR)],
            ),
            failed: last_is_failed,
            ..Default::default()
        },
        ..Default::default()
    };
    let call_info = CallInfo {
        execution: CallExecution {
            retdata: outer_retdata.clone(),
            failed: true,
            ..Default::default()
        },
        inner_calls: vec![first_inner_call_info, last_inner_call_info],
        ..Default::default()
    };
    assert_matches!(
        extract_trailing_cairo1_revert_trace(&call_info, Cairo1RevertHeader::Execution),
        Cairo1RevertSummary { stack, last_retdata, .. }
        if stack.len() == 2 && last_retdata == first_inner_retdata
    );
}

/// If extraction function is called with a successful callinfo, it should return an empty stack and
/// the original retdata.
/// We don't expect the extraction function to ever be called with a successful callinfo, but it
/// shouldn't panic anyway.
#[test]
fn test_cairo1_stack_extraction_not_failure_fallback() {
    let expected_retdata = Retdata(vec![Felt::ONE, Felt::THREE]);
    let successful_call = CallInfo {
        execution: CallExecution { retdata: expected_retdata.clone(), ..Default::default() },
        ..Default::default()
    };
    assert_matches!(
        extract_trailing_cairo1_revert_trace(&successful_call, Cairo1RevertHeader::Execution),
        Cairo1RevertSummary { stack, last_retdata, .. }
        if stack.is_empty() && last_retdata == expected_retdata
    );
}
