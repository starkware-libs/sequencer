#[starknet::contract(account)]
mod Account {
    use array::{ArrayTrait, SpanTrait};
    use starknet::account::Call;
    use starknet::info::SyscallResultTrait;
    use starknet::{ClassHash, ContractAddress, call_contract_syscall, syscalls};
    use zeroable::Zeroable;

    #[storage]
    struct Storage {}

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState, class_hash: felt252, contract_address_salt: felt252,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate_declare__(self: @ContractState, class_hash: felt252) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>,
    ) -> felt252 {
        starknet::VALIDATED
    }

    // TODO(Yoni): replace this single-call `__execute__` with the multicall-shaped
    // `multi_call` below, so this account can execute INVOKE transactions whose
    // calldata is `Array<Call>` natively (matching the standard account ABI).
    #[external(v0)]
    #[raw_output]
    fn __execute__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>,
    ) -> Span<felt252> {
        // Validate caller.
        assert(starknet::get_caller_address().is_zero(), 'INVALID_CALLER');

        call_contract_syscall(
            address: contract_address, entry_point_selector: selector, calldata: calldata.span(),
        )
            .unwrap_syscall()
    }

    /// Executes a sequence of calls and returns their concatenated return values.
    /// The intended invocation in tests is via `__execute__`, with `__execute__`
    /// forwarding `(self_address, "multi_call", serialized calls)` to this entry point.
    #[external(v0)]
    fn multi_call(self: @ContractState, mut calls: Array<Call>) -> Array<Span<felt252>> {
        let mut result = ArrayTrait::new();
        loop {
            match calls.pop_front() {
                Option::Some(call) => {
                    let res = call_contract_syscall(
                        address: call.to,
                        entry_point_selector: call.selector,
                        calldata: call.calldata,
                    )
                        .unwrap_syscall();
                    result.append(res);
                },
                Option::None => { break; },
            };
        }
        result
    }

    #[external(v0)]
    fn deploy_contract(
        self: @ContractState,
        class_hash: ClassHash,
        contract_address_salt: felt252,
        calldata: Array<felt252>,
        deploy_from_zero: bool,
    ) -> ContractAddress {
        let (address, _) = syscalls::deploy_syscall(
            class_hash, contract_address_salt, calldata.span(), deploy_from_zero,
        )
            .unwrap_syscall();
        address
    }
}

