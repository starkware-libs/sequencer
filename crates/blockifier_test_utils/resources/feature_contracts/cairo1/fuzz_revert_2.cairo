// This contract is auto-generated. To regenerate, run:
// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils test_fuzz_revert_2_almost_identical`
#[starknet::contract]
mod FuzzRevertContract {
    use core::panic_with_felt252;
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::{ContractAddress, syscalls};
    use starknet::contract_address::ContractAddressZero;
    use starknet::info::SyscallResultTrait;

    // Scenarios.
    // The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream
    // (when cairo0 fuzz contracts get the None value from the orchestrator).
    const SCENARIO_RETURN: felt252 = 0;
    const SCENARIO_CALL: felt252 = 1;

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
    }

    /// If this contract is deployed as part of the fuzz test "deploy" scenario, the orchestrator
    /// address must be provided, and run_fuzz must be non zero. Otherwise, deploy with [0,0] as
    /// args.
    #[constructor]
    fn constructor(
        ref self: ContractState, maybe_orchestrator_address: ContractAddress, run_fuzz: bool,
    ) {
        if maybe_orchestrator_address != ContractAddressZero::zero() {
            initialize(ref self, maybe_orchestrator_address);
        }
        if run_fuzz {
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
            match syscalls::call_contract_syscall(
                contract_address, FUZZ_TEST_SELECTOR, array![].span(),
            ) {
                Result::Ok(_) => (),
                Result::Err(_) => {
                    if should_unwrap_with != 0 {
                        panic_with_felt252(should_unwrap_with);
                    }
                },
            }
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
