// This contract is auto-generated. To regenerate, run:
// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils test_fuzz_revert_2_almost_identical`
#[starknet::contract]
mod FuzzRevertContract {
    use core::panic_with_felt252;
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::{ClassHash, ContractAddress, StorageAddress, SyscallResult, syscalls};
    use starknet::contract_address::ContractAddressZero;
    use starknet::info::SyscallResultTrait;

    // Scenarios.
    // The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream
    // (when cairo0 fuzz contracts get the None value from the orchestrator).
    const SCENARIO_RETURN: felt252 = 0;
    const SCENARIO_CALL: felt252 = 1;
    const SCENARIO_LIBRARY_CALL: felt252 = 2;
    const SCENARIO_WRITE: felt252 = 3;
    const SCENARIO_REPLACE_CLASS: felt252 = 4;
    const SCENARIO_DEPLOY: felt252 = 5;
    const SCENARIO_PANIC: felt252 = 6;

    const GET_INDEX_SELECTOR: felt252 = selector!("get_index");
    const SET_INDEX_SELECTOR: felt252 = selector!("set_index");
    const POP_FRONT_SELECTOR: felt252 = selector!("pop_front");
    const FUZZ_TEST_SELECTOR: felt252 = selector!("test_revert_fuzz");

    #[storage]
    struct Storage {
        orchestrator_address: ContractAddress,
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        /// Get next scenario arg from the orchestrator.
        fn pop_front(ref self: ContractState) -> felt252 {
            let orchestrator_address = self.orchestrator_address.read();
            if orchestrator_address == ContractAddressZero::zero() {
                panic!("uninitialized");
            }
            let mut result = syscalls::call_contract_syscall(
                orchestrator_address, POP_FRONT_SELECTOR, array![].span(),
            )
                .unwrap_syscall();
            *result.pop_front().unwrap()
        }

        /// Handle error-catching: innermost panic data should include the next scenario index in
        /// the orchestrator. This index must be explicitly reset as it's increments were reverted
        /// when the inner call panicked.
        fn handle_error_catch(ref self: ContractState, result: SyscallResult<Span<felt252>>) {
            match result {
                Result::Ok(_) => (),
                Result::Err(mut error) => {
                    let index: felt252 = error.pop_front().unwrap();
                    syscalls::call_contract_syscall(
                        self.orchestrator_address.read(), SET_INDEX_SELECTOR, array![index].span(),
                    )
                        .unwrap_syscall();
                },
            }
        }
    }

    /// If this contract is deployed as part of the fuzz test "deploy" scenario, the orchestrator
    /// address must be provided (the fuzz test will run automatically). Otherwise, deploy with [0]
    /// as args.
    #[constructor]
    fn constructor(ref self: ContractState, maybe_orchestrator_address: ContractAddress) {
        if maybe_orchestrator_address != ContractAddressZero::zero() {
            initialize(ref self, maybe_orchestrator_address);
            test_revert_fuzz(ref self);
        }
    }

    #[external(v0)]
    fn initialize(ref self: ContractState, orchestrator_address: ContractAddress) {
        self.orchestrator_address.write(orchestrator_address);
    }

    #[external(v0)]
    fn test_revert_fuzz(ref self: ContractState) {
        // Get next scenario; None means done.
        let scenario = self.pop_front();

        if scenario == SCENARIO_RETURN {
            return;
        }

        if scenario == SCENARIO_CALL {
            let contract_address: ContractAddress = self.pop_front().try_into().unwrap();
            let should_unwrap_with = self.pop_front();
            let result = syscalls::call_contract_syscall(
                contract_address, FUZZ_TEST_SELECTOR, array![].span(),
            );
            if should_unwrap_with != 0 {
                result.unwrap_syscall();
            } else {
                self.handle_error_catch(result);
            }
        }

        if scenario == SCENARIO_LIBRARY_CALL {
            let class_hash: ClassHash = self.pop_front().try_into().unwrap();
            let should_unwrap_with = self.pop_front();
            let result = syscalls::library_call_syscall(
                class_hash, FUZZ_TEST_SELECTOR, array![].span()
            );
            if should_unwrap_with != 0 {
                result.unwrap_syscall();
            } else {
                self.handle_error_catch(result);
            }
        }

        if scenario == SCENARIO_WRITE {
            let key: StorageAddress = self.pop_front().try_into().unwrap();
            let value = self.pop_front();
            let address_domain = 0;
            syscalls::storage_write_syscall(address_domain, key, value).unwrap_syscall();
        }

        if scenario == SCENARIO_REPLACE_CLASS {
            let class_hash: ClassHash = self.pop_front().try_into().unwrap();
            syscalls::replace_class_syscall(class_hash).unwrap_syscall();
        }

        if scenario == SCENARIO_DEPLOY {
            // The class hash is assumed to be a fuzz test class hash.
            // Deploy it with a non-trivial orchestrator address.
            let class_hash: ClassHash = self.pop_front().try_into().unwrap();
            let salt = self.pop_front();
            let deploy_from_zero: bool = true;
            let orchestrator_felt: felt252 = self.orchestrator_address.read().into();
            let ctor_calldata = array![orchestrator_felt];
            // Deploy errors cannot be caught. Just unwrap the syscall.
            syscalls::deploy_syscall(class_hash, salt, ctor_calldata.span(), deploy_from_zero)
                .unwrap_syscall();
        }

        if scenario == SCENARIO_PANIC {
            let mut current_index = syscalls::call_contract_syscall(
                self.orchestrator_address.read(), GET_INDEX_SELECTOR, array![].span(),
            )
                .unwrap_syscall();
            panic_with_felt252(*current_index.pop_front().unwrap());
        }

        // Unless explicitly stated otherwise, the next operation should be in the current call
        // context.
        test_revert_fuzz(ref self);
    }

    /// This function is here to make this contract's class hash different from the main fuzz
    /// revert contract.
    #[external(v0)]
    fn dummy_function(ref self: ContractState) -> felt252 {
        return 100;
    }
}
