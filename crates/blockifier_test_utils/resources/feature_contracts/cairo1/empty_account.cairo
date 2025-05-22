#[starknet::contract(account)]
mod Account {
    use array::{ArrayTrait, SpanTrait};
    use starknet::{ClassHash, ContractAddress, call_contract_syscall};
    use starknet::info::SyscallResultTrait;

    #[storage]
    struct Storage {}

    #[external(v0)]
    fn __validate__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    #[raw_output]
    fn __execute__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> Span<felt252> {
        call_contract_syscall(
            address: contract_address, entry_point_selector: selector, calldata: calldata.span()
        )
            .unwrap_syscall()
    }
}

