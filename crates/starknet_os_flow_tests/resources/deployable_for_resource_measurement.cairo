/// Dummy deployable account contract for deterministic deployment resource measurement.
/// Originally compiled with compiler v2.17.0-rc.4.
#[starknet::contract(account)]
mod DeployableForResourceMeasurement {
    #[storage]
    struct Storage {}

    #[external(v0)]
    fn __validate__(self: @ContractState) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState,
        class_hash: felt252,
        contract_address_salt: felt252,
        some_args: Span<felt252>,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __execute__(ref self: ContractState) {}

    #[constructor]
    fn constructor(ref self: ContractState, some_args: Span<felt252>) {}

    /// Dummy function to effect the compiled sierra and change the contract address.
    #[external(v0)]
    fn get_salt(self: @ContractState) -> felt252 {
        0
    }
}
