// This contract is auto-generated. To regenerate, run:
// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils test_fuzz_revert_2_almost_identical`
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
    /// address must be provided. Otherwise, deploy with [0] as args.
    #[constructor]
    fn constructor(ref self: ContractState, maybe_orchestrator_address: ContractAddress) {
        if maybe_orchestrator_address != ContractAddressZero::zero() {
            initialize(ref self, maybe_orchestrator_address);
        }
    }

    #[external(v0)]
    fn initialize(ref self: ContractState, orchestrator_address: ContractAddress) {
        self.orchestrator_address.write(orchestrator_address);
    }

    /// This function is here to make this contract's class hash different from the main fuzz
    /// revert contract.
    #[external(v0)]
    fn dummy_function(ref self: ContractState) -> felt252 {
        return 100;
    }
}
