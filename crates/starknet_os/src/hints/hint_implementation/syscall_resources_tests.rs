#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use apollo_starknet_os_program::test_programs::SYSCALL_RESOURCES_TEST_BYTES;
    use blockifier::test_utils::dict_state_reader::DictStateReader;
    use cairo_vm::types::builtin_name::BuiltinName;
    use cairo_vm::types::layout_name::LayoutName;
    use cairo_vm::types::relocatable::MaybeRelocatable;
    use starknet_api::executable_transaction::{
        AccountTransaction,
        InvokeTransaction as ExecutableInvokeTransaction,
        Transaction,
    };
    use starknet_api::transaction::{InvokeTransaction, InvokeTransactionV1};
    use starknet_api::transaction::TransactionHash;
    use starknet_types_core::felt::Felt;

    use crate::hint_processor::os_logger::ResourceFinalizer;
    use crate::io::os_input::OsBlockInput;
    use crate::test_utils::cairo_runner::{
        initialize_cairo_runner,
        run_cairo_0_entrypoint,
        EndpointArg,
        EntryPointRunnerConfig,
        ImplicitArg,
        ValueArg,
    };

    fn make_os_block_input_with_invoke_tx() -> OsBlockInput {
        OsBlockInput {
            transactions: vec![Transaction::Account(AccountTransaction::Invoke(
                ExecutableInvokeTransaction {
                    tx: InvokeTransaction::V1(InvokeTransactionV1::default()),
                    tx_hash: TransactionHash::default(),
                },
            ))],
            ..OsBlockInput::default()
        }
    }

    #[test]
    fn test_os_syscall_resources() {
        let os_block_input = make_os_block_input_with_invoke_tx();

        let runner_config = EntryPointRunnerConfig {
            layout: LayoutName::all_cairo,
            ..Default::default()
        };

        let implicit_args_for_init = [ImplicitArg::Builtin(BuiltinName::range_check)];
        let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
            &runner_config,
            SYSCALL_RESOURCES_TEST_BYTES,
            "measure_syscall_resources",
            &implicit_args_for_init,
            HashMap::new(),
        )
        .unwrap();

        // Allocate dummy segments for BuiltinPointers struct fields.
        // These represent the contract builtins (distinct from the OS range_check builtin runner).
        let pedersen_dummy = cairo_runner.vm.add_memory_segment();
        let contract_range_check_dummy = cairo_runner.vm.add_memory_segment();
        let ecdsa_dummy = cairo_runner.vm.add_memory_segment();
        let bitwise_dummy = cairo_runner.vm.add_memory_segment();
        let ec_op_dummy = cairo_runner.vm.add_memory_segment();
        let poseidon_dummy = cairo_runner.vm.add_memory_segment();
        let segment_arena_dummy = cairo_runner.vm.add_memory_segment();
        let range_check96_dummy = cairo_runner.vm.add_memory_segment();
        let add_mod_dummy = cairo_runner.vm.add_memory_segment();
        let mul_mod_dummy = cairo_runner.vm.add_memory_segment();
        let keccak_dummy = cairo_runner.vm.add_memory_segment();
        let sha256_dummy = cairo_runner.vm.add_memory_segment();

        // Write BuiltinPointers struct: selectable (10 fields) + non_selectable (2 fields).
        // Field order matches SelectableBuiltins then NonSelectableBuiltins layout.
        let builtin_ptrs_seg = cairo_runner.vm.add_memory_segment();
        let builtin_ptrs_data: Vec<MaybeRelocatable> = vec![
            pedersen_dummy.into(),
            contract_range_check_dummy.into(),
            ecdsa_dummy.into(),
            bitwise_dummy.into(),
            ec_op_dummy.into(),
            poseidon_dummy.into(),
            segment_arena_dummy.into(),
            range_check96_dummy.into(),
            add_mod_dummy.into(),
            mul_mod_dummy.into(),
            keccak_dummy.into(),
            sha256_dummy.into(),
        ];
        cairo_runner.vm.load_data(builtin_ptrs_seg, &builtin_ptrs_data).unwrap();

        // Allocate a segment for the failure message written by write_failure_response.
        let failure_msg_seg = cairo_runner.vm.add_memory_segment();

        // Empty calldata segment (calldata_start == calldata_end).
        let calldata_seg = cairo_runner.vm.add_memory_segment();

        // Syscall segment layout for CallContract with gas=0:
        //   [0] RequestHeader.selector = CALL_CONTRACT_SELECTOR
        //   [1] RequestHeader.gas = 0
        //   [2] CallContractRequest.contract_address
        //   [3] CallContractRequest.selector
        //   [4] CallContractRequest.calldata_start
        //   [5] CallContractRequest.calldata_end
        //   [6] ResponseHeader.gas          (written by write_failure_response)
        //   [7] ResponseHeader.failure_flag (written by write_failure_response)
        //   [8] FailureReason.start         (pre-initialized; write_failure_response reads & writes through it)
        //   [9] FailureReason.end           (written by write_failure_response)
        let call_contract_selector = Felt::from_bytes_be_slice(b"CallContract");
        let syscall_seg = cairo_runner.vm.add_memory_segment();
        let syscall_request: Vec<MaybeRelocatable> = vec![
            call_contract_selector.into(),
            Felt::ZERO.into(),
            Felt::ZERO.into(),
            Felt::ZERO.into(),
            calldata_seg.into(),
            calldata_seg.into(),
        ];
        cairo_runner.vm.load_data(syscall_seg, &syscall_request).unwrap();
        // Pre-initialize failure_reason.start at offset 8 (offsets 6-7 left for the VM to write).
        cairo_runner.vm.insert_value((syscall_seg + 8usize).unwrap(), failure_msg_seg).unwrap();

        // Dummy segments for implicit args not accessed in the gas=0 path.
        let contract_state_changes_seg = cairo_runner.vm.add_memory_segment();
        let contract_class_changes_seg = cairo_runner.vm.add_memory_segment();
        let revert_log_seg = cairo_runner.vm.add_memory_segment();
        let outputs_seg = cairo_runner.vm.add_memory_segment();

        // syscall_ptr_end points past the last field of the response (offset 10).
        let syscall_ptr_end = (syscall_seg + 10usize).unwrap();

        // Dummy segments for block_context and execution_context (not accessed for gas=0).
        let block_context_seg = cairo_runner.vm.add_memory_segment();
        let execution_context_seg = cairo_runner.vm.add_memory_segment();

        let implicit_args = [
            ImplicitArg::Builtin(BuiltinName::range_check),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(syscall_seg.into()))),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(
                builtin_ptrs_seg.into(),
            ))),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(
                contract_state_changes_seg.into(),
            ))),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(
                contract_class_changes_seg.into(),
            ))),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(revert_log_seg.into()))),
            ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(outputs_seg.into()))),
        ];

        let explicit_args = [
            EndpointArg::Value(ValueArg::Single(block_context_seg.into())),
            EndpointArg::Value(ValueArg::Single(execution_context_seg.into())),
            EndpointArg::Value(ValueArg::Single(syscall_ptr_end.into())),
        ];

        let (_implicit_return_values, _explicit_return_values, hint_processor) =
            run_cairo_0_entrypoint(
                entrypoint,
                &explicit_args,
                &implicit_args,
                Some(DictStateReader::default()),
                &mut cairo_runner,
                &program,
                &runner_config,
                &[],
                &os_block_input,
            )
            .unwrap();

        let txs = hint_processor
            .execution_helpers_manager
            .get_current_execution_helper()
            .unwrap()
            .os_logger
            .get_txs();

        assert_eq!(txs.len(), 1);
        let syscall_trace = &txs[0].syscalls[0];
        let resources = syscall_trace.get_resources().unwrap();

        assert_eq!(
            resources.builtin_instance_counter.get(&BuiltinName::range_check).copied().unwrap_or(0),
            1,
            "CallContract OOG path should use exactly 1 range_check cell (assert_lt)"
        );
        assert!(resources.n_steps > 0, "Expected non-zero step count");
    }
}
