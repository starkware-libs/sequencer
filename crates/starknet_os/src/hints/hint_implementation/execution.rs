use blockifier::abi::constants;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use starknet_api::block::BlockNumber;

use crate::hints::error::{HintResult, OsHintError};
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

pub(crate) fn load_next_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn exit_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn prepare_constructor_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn transaction_version<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn assert_transaction_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_deprecated_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_state_entry_to_account_contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_block_hash_contract_address_state_entry_and_set_new_state_entry<
    S: StateReader,
>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry_and_set_new_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn check_is_deprecated<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn is_deprecated<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn enter_syscall_scopes<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn end_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   end_tx().
    todo!()
}

pub(crate) fn enter_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   enter_call().
    todo!()
}

pub(crate) fn exit_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   exit_call().
    todo!()
}

pub(crate) fn contract_address<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_calldata_len<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_calldata<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_entry_point_selector<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn tx_max_fee<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_tip<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn tx_nonce_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn tx_fee_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn gen_signature_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn is_reverted<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn check_execution<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn is_remaining_gas_lt_initial_budget<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn check_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn check_new_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn check_new_deploy_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn log_enter_syscall<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn initial_ge_required_gas<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_ap_to_tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_fp_plus_4_to_tx_nonce<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_new_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_0<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_1<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_left_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_right_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn enter_scope_descend_edge<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn write_syscall_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn gen_class_hash_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn write_old_block_to_storage<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> HintResult {
    todo!()
}

// pub const GET_OLD_BLOCK_NUMBER_AND_HASH: &str = indoc! {r#"
// 	(
// 	    old_block_number, old_block_hash
// 	) = execution_helper.get_old_block_number_and_hash()
// 	assert old_block_number == ids.old_block_number,(
// 	    "Inconsistent block number. "
// 	    "The constant STORED_BLOCK_HASH_BUFFER is probably out of sync."
// 	)
// 	ids.old_block_hash = old_block_hash"#
// };

// pub async fn get_old_block_number_and_hash_async<PCS>(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
// ) -> Result<(), HintError>
// where
//     PCS: PerContractStorage + 'static,
// {
//     let execution_helper: ExecutionHelperWrapper<PCS> =
//         exec_scopes.get(vars::scopes::EXECUTION_HELPER)?;
//     let (old_block_number, old_block_hash) =
//         execution_helper.get_old_block_number_and_hash().await?;

//     let ids_old_block_number =
//         get_integer_from_var_name(vars::ids::OLD_BLOCK_NUMBER, vm, ids_data, ap_tracking)?;
//     if old_block_number != ids_old_block_number {
//         log::warn!(
//             "old_block_number ({}) != ids_old_block_number ({})",
//             old_block_number,
//             ids_old_block_number
//         );
//         return Err(HintError::AssertionFailed(
//             "Inconsistent block number. The constant STORED_BLOCK_HASH_BUFFER is probably out of
// \              sync."
//                 .to_string()
//                 .into_boxed_str(),
//         ));
//     }

//     insert_value_from_var_name(
//         vars::ids::OLD_BLOCK_HASH,
//         old_block_hash,
//         vm,
//         ids_data,
//         ap_tracking,
//     )?;

//     Ok(())
// }

// pub fn get_old_block_number_and_hash<PCS>(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError>
// where
//     PCS: PerContractStorage + 'static,
// {
//     execute_coroutine(get_old_block_number_and_hash_async::<PCS>(
//         vm,
//         exec_scopes,
//         ids_data,
//         ap_tracking,
//     ))?
// }

pub(crate) fn get_old_block_number_and_hash<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> HintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let (old_block_number, old_block_hash) =
        os_input.old_block_number_and_hash.unwrap_or_else(|| {
            panic!("Block number is probably < {0}.", constants::STORED_BLOCK_HASH_BUFFER)
        });
    let ids_old_block_number = BlockNumber(
        get_integer_from_var_name(Ids::OldBlockNumber.into(), vm, ids_data, ap_tracking)?
            .try_into()
            .expect("Block number should fit in u64"),
    );
    if old_block_number != ids_old_block_number {
        return Err(OsHintError::InconsistentBlockNumber {
            expected: old_block_number,
            actual: ids_old_block_number,
        });
    }
    insert_value_from_var_name(
        Ids::OldBlockHash.into(),
        old_block_hash.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn fetch_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
