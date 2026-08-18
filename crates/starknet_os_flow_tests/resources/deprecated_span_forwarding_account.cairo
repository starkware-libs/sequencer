// A minimal account whose constructor forwards the (logically empty) `account_deployment_data`
// span from the current transaction info into a Cairo0 proxy via `library_call`.
//
// DeployAccount V3 has no account-deployment data, so the OS builds that span with felt-zero
// endpoints (`cast(0, felt*)`). Forwarding it into a Cairo0 (deprecated) contract makes the OS
// deliver a null calldata pointer, which the proxy re-emits through a deprecated syscall. The
// deprecated `read_felt_array` then dereferences that null pointer: native Blockifier (real
// segment) succeeds, while an unfixed OS aborts with "Expected relocatable" (STARKNET-96, reached
// via the deprecated parser). Used as a regression fixture.
//
// Constructor calldata: `[proxy_class_hash]` — the class hash of the Cairo0 proxy to library_call.
#[starknet::contract(account)]
mod Account {
    use array::{ArrayTrait, SpanTrait};
    use box::BoxTrait;
    use option::OptionTrait;
    use starknet::class_hash::ClassHash;
    use starknet::syscalls::library_call_syscall;
    use starknet::info::SyscallResultTrait;
    use starknet::ContractAddress;
    use traits::TryInto;

    #[storage]
    struct Storage {}

    #[constructor]
    fn constructor(ref self: ContractState, proxy_class_hash: felt252) {
        let execution_info = starknet::get_execution_info().unbox();
        let tx_info = execution_info.tx_info.unbox();
        let class_hash: ClassHash = proxy_class_hash.try_into().unwrap();
        // The Cairo0 proxy's raw-input entrypoint forwards this calldata into a deprecated syscall.
        library_call_syscall(class_hash, 0, tx_info.account_deployment_data).unwrap_syscall();
    }

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState,
        class_hash: felt252,
        contract_address_salt: felt252,
        proxy_class_hash: felt252,
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

    #[external(v0)]
    #[raw_output]
    fn __execute__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>,
    ) -> Span<felt252> {
        let empty: Array<felt252> = ArrayTrait::new();
        empty.span()
    }
}
