// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{call_contract_syscall, deploy_syscall, library_call_syscall};
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

        // library_call syscall — calls empty_function on stable class hash.
        library_call_syscall(
            stable_class_hash, STABLE_EXTERNAL_ENTRY_POINT_SELECTOR, array![0].span(),
        )
            .unwrap_syscall();

        // deploy syscall. The resources this syscall consumes can vary depending on the deployed
        // contract address, in a non-trivial way (see `normalize_address` in the cairo0 core). For
        // this reason we deploy from zero, and choose a specific salt.
        // base (no calldata):
        deploy_syscall(stable_class_hash, 3, array![0].span(), true).unwrap_syscall();
        // linear factor (calldata len = 1):
        deploy_syscall(stable_class_hash, 3, array![1, 0].span(), true).unwrap_syscall();
    }
}
