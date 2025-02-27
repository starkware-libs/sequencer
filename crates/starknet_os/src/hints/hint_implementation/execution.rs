use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_constant_from_var_name,
    get_integer_from_var_name,
    insert_value_from_var_name,
};
use starknet_api::block::BlockNumber;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{Const, Ids};

pub(crate) fn load_next_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn exit_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn prepare_constructor_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn transaction_version<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_transaction_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_deprecated_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_state_entry_to_account_contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_hash_contract_address_state_entry_and_set_new_state_entry<
    S: StateReader,
>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry_and_set_new_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_is_deprecated<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn is_deprecated<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_syscall_scopes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn end_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   end_tx().
    todo!()
}

pub(crate) fn enter_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   enter_call().
    todo!()
}

pub(crate) fn exit_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   exit_call().
    todo!()
}

pub(crate) fn contract_address<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_calldata_len<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_calldata<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_entry_point_selector<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_max_fee<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_tip<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_nonce_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_fee_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn gen_signature_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_reverted<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn check_execution<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_remaining_gas_lt_initial_budget<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_deploy_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn log_enter_syscall<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn initial_ge_required_gas<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_fp_plus_4_to_tx_nonce<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_new_node<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_0<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_1<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_left_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_right_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_descend_edge<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_syscall_result<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn gen_class_hash_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn write_old_block_to_storage<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_old_block_number_and_hash<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    match os_input.old_block_number_and_hash {
        None => {
            let stored_block_hash_buffer =
                get_constant_from_var_name(Const::StoredBlockHashBuffer.into(), constants)?;
            Err(OsHintError::TooSmallBlockNumber {
                stored_block_hash_buffer: *stored_block_hash_buffer,
            })
        }
        Some((old_block_number, old_block_hash)) => {
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
    }
}

pub(crate) fn fetch_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
