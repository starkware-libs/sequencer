#[starknet::interface]
trait IOrchestrator<TContractState> {
    fn pop_front(ref self: TContractState) -> felt252;
    fn get_index(ref self: TContractState) -> felt252;
    fn set_index(ref self: TContractState, index: felt252);
    fn should_fail_undeployed_panic_message(ref self: TContractState) -> felt252;
}

#[starknet::contract]
mod FuzzRevertContract {
    use super::IOrchestratorDispatcher;
    use super::IOrchestratorDispatcherTrait;
    use core::{keccak, panic_with_felt252};
    use core::sha256::compute_sha256_u32_array;
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::{ClassHash, ContractAddress, StorageAddress, SyscallResult, syscalls};
    use starknet::contract_address::ContractAddressZero;
    use starknet::info::SyscallResultTrait;

    // Scenarios.
    // The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream
    // (when cairo0 fuzz contracts get the None value from the orchestrator).
    // TODO(Dori): Convert to enum.
    const SCENARIO_RETURN: felt252 = 0;
    const SCENARIO_CALL: felt252 = 1;
    const SCENARIO_LIBRARY_CALL: felt252 = 2;
    const SCENARIO_WRITE: felt252 = 3;
    const SCENARIO_REPLACE_CLASS: felt252 = 4;
    const SCENARIO_DEPLOY: felt252 = 5;
    const SCENARIO_PANIC: felt252 = 6;
    const SCENARIO_INCREMENT_COUNTER: felt252 = 7;
    const SCENARIO_SEND_MESSAGE: felt252 = 8;
    const SCENARIO_DEPLOY_NON_EXISTING: felt252 = 9;
    const SCENARIO_LIBRARY_CALL_NON_EXISTING: felt252 = 10;
    const SCENARIO_SHA256: felt252 = 11;
    const SCENARIO_KECCAK: felt252 = 12;
    const SCENARIO_CALL_UNDEPLOYED: felt252 = 13;

    #[storage]
    struct Storage {
        counter: felt252,
        orchestrator_address: ContractAddress,
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        fn orchestrator(ref self: ContractState) -> IOrchestratorDispatcher {
            let orchestrator_address = self.orchestrator_address.read();
            assert(orchestrator_address != ContractAddressZero::zero(), 'uninitialized');
            IOrchestratorDispatcher { contract_address: orchestrator_address }
        }

        /// Handle error-catching: innermost panic data should include the next scenario index in
        /// the orchestrator. This index must be explicitly reset as it's increments were reverted
        /// when the inner call panicked.
        fn handle_error_catch(
            ref self: ContractState, result: SyscallResult<Span<felt252>>, should_unwrap: bool
        ) {
            if should_unwrap {
                result.unwrap_syscall();
            } else {
                match result {
                    Result::Ok(_) => (),
                    Result::Err(mut error) => self
                        .orchestrator()
                        .set_index(error.pop_front().unwrap()),
                }
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
        self.counter.write(0xc10);
        self.orchestrator_address.write(orchestrator_address);
    }

    #[external(v0)]
    fn test_revert_fuzz(ref self: ContractState) {
        let orchestrator = self.orchestrator();

        // Get next scenario; None means done.
        let scenario = orchestrator.pop_front();

        if scenario == SCENARIO_RETURN {
            return;
        }

        if scenario == SCENARIO_CALL {
            let address: ContractAddress = orchestrator.pop_front().try_into().unwrap();
            let selector = orchestrator.pop_front();
            let should_unwrap: bool = orchestrator.pop_front() != 0;
            let result = syscalls::call_contract_syscall(address, selector, array![].span());
            self.handle_error_catch(result, should_unwrap);
        }

        if scenario == SCENARIO_LIBRARY_CALL {
            let class_hash: ClassHash = orchestrator.pop_front().try_into().unwrap();
            let selector = orchestrator.pop_front();
            let should_unwrap: bool = orchestrator.pop_front() != 0;
            let result = syscalls::library_call_syscall(class_hash, selector, array![].span());
            self.handle_error_catch(result, should_unwrap);
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
            let ctor_calldata = array![self.orchestrator_address.read().into()];
            // Deploy errors cannot be caught. Just unwrap the syscall.
            syscalls::deploy_syscall(class_hash, salt, ctor_calldata.span(), deploy_from_zero)
                .unwrap_syscall();
        }

        if scenario == SCENARIO_PANIC {
            // Panic message is part of the scenario data.
            let message = orchestrator.pop_front();
            panic(array![orchestrator.get_index(), message]);
        }

        if scenario == SCENARIO_INCREMENT_COUNTER {
            let value = self.counter.read();
            self.counter.write(value + 1);
        }

        if scenario == SCENARIO_SEND_MESSAGE {
            let payload = array![orchestrator.pop_front()];
            syscalls::send_message_to_l1_syscall(0xadd1, payload.span()).unwrap_syscall();
        }

        if scenario == SCENARIO_DEPLOY_NON_EXISTING {
            let class_hash: ClassHash = 0xde6107000c1.try_into().unwrap();
            let salt = 0;
            let deploy_from_zero: bool = true;
            // Unrecoverable error (we do not prove class hashes do not exist), no option to catch
            // error.
            syscalls::deploy_syscall(class_hash, salt, array![].span(), deploy_from_zero)
                .unwrap_syscall();
        }

        if scenario == SCENARIO_LIBRARY_CALL_NON_EXISTING {
            let class_hash: ClassHash = 0x11bca11000c1.try_into().unwrap();
            // Unrecoverable error (we do not prove class hashes do not exist), no option to catch
            // error.
            syscalls::library_call_syscall(class_hash, 0, array![].span()).unwrap_syscall();
        }

        if scenario == SCENARIO_SHA256 {
            let preimage: u32 = orchestrator.pop_front().try_into().unwrap();
            compute_sha256_u32_array(array![preimage], 0, 0);
        }

        if scenario == SCENARIO_KECCAK {
            let preimage: u128 = orchestrator.pop_front().try_into().unwrap();
            let mut input: Array::<u256> = Default::default();
            input.append(u256 { low: preimage, high: preimage });
            keccak::keccak_u256s_le_inputs(input.span());
        }

        if scenario == SCENARIO_CALL_UNDEPLOYED {
            let address: ContractAddress = orchestrator.pop_front().try_into().unwrap();
            let selector = orchestrator.pop_front();
            let _should_unwrap = orchestrator.pop_front();
            // Calling an undeployed contract should be an uncatchable fail.
            syscalls::call_contract_syscall(address, selector, array![].span()).unwrap_err();
            panic_with_felt252(orchestrator.should_fail_undeployed_panic_message());
        }

        // Unless explicitly stated otherwise, the next operation should be in the current call
        // context.
        test_revert_fuzz(ref self);
    }
}
