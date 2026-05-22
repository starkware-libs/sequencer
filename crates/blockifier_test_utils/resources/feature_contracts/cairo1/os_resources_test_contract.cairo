// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{call_contract_syscall, deploy_syscall, library_call_syscall};
    use starknet::{ClassHash, ContractAddress};

    const EMPTY_FUNCTION_SELECTOR: felt252 = selector!("empty_function");

    #[storage]
    struct Storage {}

    #[external(v0)]
    fn __validate_declare__(
        self: @ContractState, class_hash: ClassHash, self_address: ContractAddress,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(
        self: @ContractState,
        self_class_hash: ClassHash,
        self_address: ContractAddress,
        deployable_class_hash: ClassHash,
    ) -> felt252 {
        starknet::VALIDATED
    }

    // Calls every measured syscall in order.
    #[external(v0)]
    fn __execute__(
        ref self: ContractState,
        self_class_hash: ClassHash,
        self_address: ContractAddress,
        deployable_class_hash: ClassHash,
    ) {
        // call_contract syscall — calls empty_function on self.
        call_contract_syscall(self_address, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span())
            .unwrap_syscall();

        // library_call syscall — calls empty_function on self.
        library_call_syscall(self_class_hash, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span())
            .unwrap_syscall();

        // deploy syscall. The resources this syscall consumes can vary depending on the deployed
        // contract address, in a non-trivial way (see `normalize_address` in the cairo0 core). For
        // this reason we deploy from zero, and choose a specific salt.
        // The deployed class hash is not expected to change (the class hash of a fixed, precompiled
        // contract).
        // base (no calldata):
        deploy_syscall(deployable_class_hash, 2, array![0].span(), true).unwrap_syscall();
        // linear factor (calldata len = 1):
        deploy_syscall(deployable_class_hash, 2, array![1, 0].span(), true).unwrap_syscall();
    }

    // Target for call_contract and library_call — accepts no arguments.
    #[external(v0)]
    fn empty_function(self: @ContractState) {}
}
