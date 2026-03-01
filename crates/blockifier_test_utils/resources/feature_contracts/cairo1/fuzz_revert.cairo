#[starknet::contract]
mod FuzzRevertContract {
    use starknet::storage::StoragePointerWriteAccess;
    use starknet::ContractAddress;
    use starknet::contract_address::ContractAddressZero;

    #[storage]
    struct Storage {
        orchestrator_address: ContractAddress,
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
    fn test_revert_fuzz(ref self: ContractState) {}
}
