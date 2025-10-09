#[starknet::contract(account)]
mod ExperimentalContract {
    // The purpose of this contract is to test libfuncs not enabled by all the tests.
    use array::{ArrayTrait, SpanTrait};
    use box::BoxTrait;
    use starknet::{
        ClassHash, ContractAddress, call_contract_syscall, deploy_syscall, info::SyscallResultTrait
    };
    use core::testing::get_available_gas;

    #[storage]
    struct Storage {}

    #[constructor]
    fn constructor(ref self: ContractState) {
        return;
    }

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState, class_hash: ClassHash, contract_address_salt: felt252
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate_declare__(self: @ContractState, class_hash: ClassHash) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(
        ref self: ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> felt252 {
        if (selector == selector!("verify_gas_limits")) {
            let expected_validate_gas_limit_min = *calldata.span()[0];
            let expected_validate_gas_limit_max = *calldata.span()[1];
            verify_gas_limits(
                ref self, 0, 0, expected_validate_gas_limit_min, expected_validate_gas_limit_max
            );
        }
        starknet::VALIDATED
    }

    #[external(v0)]
    fn verify_gas_limits(
        ref self: ContractState,
        _validate_placeholder1: felt252,
        _validate_placeholder2: felt252,
        expected_gas_limit_min: felt252,
        expected_gas_limit_max: felt252,
    ) {
        let available_gas = get_available_gas();
        core::gas::withdraw_gas().unwrap();
        let gas_in_bounds = available_gas < expected_gas_limit_max.try_into().unwrap()
            && available_gas > expected_gas_limit_min.try_into().unwrap();
        assert(gas_in_bounds, 'GAS_NOT_IN_BOUNDS');
    }

    #[l1_handler]
    fn verify_gas_limits_l1_handler(
        ref self: ContractState,
        from_address: felt252,
        expected_l1_handler_gas_limit_min: felt252,
        expected_l1_handler_gas_limit_max: felt252,
    ) {
        verify_gas_limits(
            ref self, 0, 0, expected_l1_handler_gas_limit_min, expected_l1_handler_gas_limit_max
        );
    }

    #[external(v0)]
    #[raw_output]
    fn __execute__(
        ref self: ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> Span<felt252> {
        starknet::call_contract_syscall(
            address: contract_address, entry_point_selector: selector, calldata: calldata.span()
        )
            .unwrap_syscall()
    }
}
