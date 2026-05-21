// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::call_contract_syscall;
    use starknet::{ClassHash, ContractAddress};

    const STABLE_EXTERNAL_ENTRY_POINT_SELECTOR: felt252 = selector!("external");

    #[storage]
    struct Storage {}

    #[external(v0)]
    fn __validate_declare__(
        self: @ContractState, stable_class_hash: ClassHash, stable_address: ContractAddress,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(
        self: @ContractState, stable_class_hash: ClassHash, stable_address: ContractAddress,
    ) -> felt252 {
        starknet::VALIDATED
    }

    // Calls every measured syscall in order.
    #[external(v0)]
    fn __execute__(
        ref self: ContractState, stable_class_hash: ClassHash, stable_address: ContractAddress,
    ) {
        // call_contract syscall — calls external function on stable contract.
        call_contract_syscall(
            stable_address, STABLE_EXTERNAL_ENTRY_POINT_SELECTOR, array![0].span(),
        )
            .unwrap_syscall();
    }
}
