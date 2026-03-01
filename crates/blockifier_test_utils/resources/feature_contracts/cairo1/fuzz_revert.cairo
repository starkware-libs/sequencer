#[starknet::contract]
mod FuzzRevertContract {
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::{ContractAddress, syscalls};
    use starknet::contract_address::ContractAddressZero;
    use starknet::info::SyscallResultTrait;

    // Scenarios.
    // The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream
    // (when cairo0 fuzz contracts get the None value from the orchestrator).
    const SCENARIO_RETURN: felt252 = 0;

    const POP_FRONT_SELECTOR: felt252 = selector!("pop_front");

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

        // Unless explicitly stated otherwise, the next operation should be in the current call
        // context.
        test_revert_fuzz(ref self);
    }
}
