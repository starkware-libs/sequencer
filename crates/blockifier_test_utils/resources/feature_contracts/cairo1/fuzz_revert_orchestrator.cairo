#[starknet::contract]
mod FuzzRevertOrchestratorContract {
    use starknet::{ContractAddress, syscalls};
    use starknet::storage::{
        MutableVecTrait, StoragePointerReadAccess, StoragePointerWriteAccess, Vec, VecTrait,
    };

    const UNEXPECTED_FAIL_INVALID_SCENARIO: felt252 = 'invalid_scenario';
    const OOB_ERROR: felt252 = 'index_OOB';
    const UNEXPECTED_FAIL_UNDEPLOYED: felt252 = 'should_fail_undeployed';
    const UNEXPECTED_FAIL_CALL_NO_ENTRYPOINT: felt252 = 'call_no_entrypoint';
    const UNEXPECTED_FAIL_LIBCALL_NO_ENTRYPOINT: felt252 = 'libcall_no_entrypoint';

    #[storage]
    struct Storage {
        scenarios: Vec<felt252>,
        front_index: u64,
    }

    #[external(v0)]
    fn initialize(ref self: ContractState, scenarios: Array<felt252>) {
        assert(self.scenarios.len() == 0, 'already_initialized');
        for scenario in scenarios {
            self.scenarios.push(scenario);
        }
        self.front_index.write(0);
    }

    /// Get next scenario arg from the orchestrator. If there are no more scenario args, assume the
    /// caller is requesting the next scenario identifier, and return the RETURN scenario.
    #[external(v0)]
    fn pop_front(ref self: ContractState) -> felt252 {
        let front = self.front_index.read();
        if front >= self.scenarios.len() {
            return 0; // RETURN scenario.
        }
        let value = self.scenarios.at(front).read();
        self.front_index.write(front + 1);
        value
    }

    #[external(v0)]
    fn get_index(ref self: ContractState) -> felt252 {
        self.front_index.read().into()
    }

    /// Set the index of the next scenario felt.
    /// Used when an error is caught from a parent context - the inner call state changes are
    /// reverted, so to continue the test from the next scenario, the pre-revert index is propagated
    /// as the error value.
    /// If this entry point is called with an index greater than the number of scenarios, the test
    /// will panic (this can happen if an unexpected error occurs and is propagated).
    #[external(v0)]
    fn set_index(ref self: ContractState, index: felt252) {
        let index_u64: u64 = match index.try_into() {
            Option::Some(val) => val,
            Option::None => panic_with_felt252(OOB_ERROR),
        };
        assert(index_u64 <= self.scenarios.len(), OOB_ERROR);
        self.front_index.write(index_u64);
    }

    /// Expose the unexpected panic messages to the fuzz test contract(s), so they know what to
    /// panic with in case of unexpected failures.

    #[external(v0)]
    fn should_fail_invalid_scenario_panic_message(ref self: ContractState) -> felt252 {
        UNEXPECTED_FAIL_INVALID_SCENARIO
    }

    #[external(v0)]
    fn should_fail_undeployed_panic_message(ref self: ContractState) -> felt252 {
        UNEXPECTED_FAIL_UNDEPLOYED
    }

    #[external(v0)]
    fn should_fail_call_no_entrypoint_panic_message(ref self: ContractState) -> felt252 {
        UNEXPECTED_FAIL_CALL_NO_ENTRYPOINT
    }

    #[external(v0)]
    fn should_fail_libcall_no_entrypoint_panic_message(ref self: ContractState) -> felt252 {
        UNEXPECTED_FAIL_LIBCALL_NO_ENTRYPOINT
    }

    /// Start the test. The first address must be an initialized fuzz test contract.
    #[external(v0)]
    fn start_test(ref self: ContractState, first_address: ContractAddress) {
        match syscalls::call_contract_syscall(
            first_address, selector!("test_revert_fuzz"), array![].span(),
        ) {
            Result::Ok(_) => (),
            Result::Err(mut error) => {
                // Assert the error is not any unexpected error.
                let error_value = error.pop_front().unwrap();
                for unexpected_error in array![
                    UNEXPECTED_FAIL_INVALID_SCENARIO,
                    OOB_ERROR,
                    UNEXPECTED_FAIL_UNDEPLOYED,
                    UNEXPECTED_FAIL_CALL_NO_ENTRYPOINT,
                    UNEXPECTED_FAIL_LIBCALL_NO_ENTRYPOINT
                ]
                    .span() {
                        assert(error_value != *unexpected_error, *unexpected_error);
                    }
            }
        }
    }
}
