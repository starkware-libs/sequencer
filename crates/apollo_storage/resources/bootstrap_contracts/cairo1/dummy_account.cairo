#[starknet::contract(account)]
mod account {
    use array::{ArrayTrait, SpanTrait};
    use box::BoxTrait;
    use starknet::{
        ClassHash, ContractAddress, call_contract_syscall, deploy_syscall, replace_class_syscall,
        info::SyscallResultTrait
    };

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
        starknet::VALIDATED
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


    #[l1_handler]
    fn empty_l1_handler(self: @ContractState, from_address: felt252) {
        return;
    }

    #[external(v0)]
    fn deploy_contract(
        self: @ContractState,
        class_hash: ClassHash,
        contract_address_salt: felt252,
        calldata: Array::<felt252>,
    ) -> (ContractAddress, Span<felt252>) {
        // Verify caller.
        let execution_info = starknet::get_execution_info().unbox();
        assert(execution_info.caller_address == execution_info.contract_address, 'INVALID_CALLER');

        deploy_syscall(class_hash, contract_address_salt, calldata.span(), false).unwrap_syscall()
    }

    #[external(v0)]
    fn execute_replace_class(ref self: ContractState, class_hash: ClassHash) {
        replace_class_syscall(class_hash).unwrap_syscall()
    }
}
