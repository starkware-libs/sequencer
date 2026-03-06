// This contract is auto-generated. To regenerate, run:
// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils test_fuzz_revert_2_almost_identical`
#[starknet::interface]
trait IOrchestrator<TContractState> {
    fn pop_front(ref self: TContractState) -> felt252;
}

#[starknet::contract]
mod FuzzRevertContract {
    use super::IOrchestratorDispatcher;
    use super::IOrchestratorDispatcherTrait;
    use core::panic_with_felt252;
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::{ClassHash, ContractAddress, StorageAddress, syscalls};
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

    const FUZZ_TEST_SELECTOR: felt252 = selector!("test_revert_fuzz");

    #[storage]
    struct Storage {
        orchestrator_address: ContractAddress,
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
        let orchestrator_address = self.orchestrator_address.read();
        assert(orchestrator_address != ContractAddressZero::zero(), 'uninitialized');
        let orchestrator = IOrchestratorDispatcher { contract_address: orchestrator_address };
        let scenario = orchestrator.pop_front();

        if scenario == SCENARIO_RETURN {
            return;
        }

        if scenario == SCENARIO_CALL {
            let contract_address: ContractAddress = orchestrator.pop_front().try_into().unwrap();
            let should_unwrap_with = orchestrator.pop_front();
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

        if scenario == SCENARIO_LIBRARY_CALL {
            let class_hash: ClassHash = orchestrator.pop_front().try_into().unwrap();
            let should_unwrap_with = orchestrator.pop_front();
            match syscalls::library_call_syscall(class_hash, FUZZ_TEST_SELECTOR, array![].span()) {
                Result::Ok(_) => (),
                Result::Err(_) => {
                    if should_unwrap_with != 0 {
                        panic_with_felt252(should_unwrap_with);
                    }
                },
            }
        }

        if scenario == SCENARIO_WRITE {
            let key: StorageAddress = orchestrator.pop_front().try_into().unwrap();
            let value = orchestrator.pop_front();
            let address_domain = 0;
            syscalls::storage_write_syscall(address_domain, key, value).unwrap_syscall();
        }

        if scenario == SCENARIO_REPLACE_CLASS {
            let class_hash: ClassHash = orchestrator.pop_front().try_into().unwrap();
            syscalls::replace_class_syscall(class_hash).unwrap_syscall();
        }

        if scenario == SCENARIO_DEPLOY {
            // The class hash is assumed to be a fuzz test class hash.
            // Deploy it with a non-trivial orchestrator address.
            let class_hash: ClassHash = orchestrator.pop_front().try_into().unwrap();
            let salt = orchestrator.pop_front();
            let deploy_from_zero: bool = true;
            let ctor_calldata = array![orchestrator_address.into()];
            // Deploy errors cannot be caught. Just unwrap the syscall.
            syscalls::deploy_syscall(class_hash, salt, ctor_calldata.span(), deploy_from_zero)
                .unwrap_syscall();
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
