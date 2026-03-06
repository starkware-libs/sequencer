#[starknet::contract]
mod FuzzRevertOrchestratorContract {
    use starknet::{ContractAddress, syscalls};
    use starknet::storage::{
        MutableVecTrait, StoragePointerReadAccess, StoragePointerWriteAccess, Vec, VecTrait,
    };

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

    #[external(v0)]
    fn set_index(ref self: ContractState, index: felt252) {
        self.front_index.write(index.try_into().unwrap());
    }

    /// Start the test. The first address must be an initialized fuzz test contract.
    #[external(v0)]
    fn start_test(ref self: ContractState, first_address: ContractAddress) {
        match syscalls::call_contract_syscall(
            first_address, selector!("test_revert_fuzz"), array![].span(),
        ) {
            Result::Ok(_) | Result::Err(_) => (),
        }
    }
}
