// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{
        call_contract_syscall, deploy_syscall, emit_event_syscall, library_call_syscall,
    };
    use starknet::{ClassHash, ContractAddress};

    const STABLE_EXTERNAL_ENTRY_POINT_SELECTOR: felt252 = selector!("external");
    const EXECUTE_FUNCTION_SELECTOR: felt252 = selector!("__execute__");

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

    extern fn meta_tx_v0_syscall(
        address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Span<felt252>,
        signature: Span<felt252>,
    ) -> starknet::SyscallResult<Span<felt252>> implicits(GasBuiltin, System) nopanic;

    // Calls every measured syscall in order.
    #[external(v0)]
    fn __execute__(
        ref self: ContractState, stable_class_hash: ClassHash, stable_address: ContractAddress,
    ) {
        // Skip everything if inputs are zero.
        if stable_class_hash.is_zero() && stable_address.is_zero() {
            return;
        }

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

        // meta_tx_v0 syscall - base.
        meta_tx_v0_syscall(
            address: stable_address,
            entry_point_selector: EXECUTE_FUNCTION_SELECTOR,
            calldata: array![0].span(),
            signature: array![].span(),
        )
            .unwrap_syscall();
        // meta_tx_v0 syscall - linear factor.
        meta_tx_v0_syscall(
            address: stable_address,
            entry_point_selector: EXECUTE_FUNCTION_SELECTOR,
            calldata: array![1, 0].span(),
            signature: array![].span(),
        )
            .unwrap_syscall();

        // deploy syscall. The resources this syscall consumes can vary depending on the deployed
        // contract address, in a non-trivial way (see `normalize_address` in the cairo0 core). For
        // this reason we deploy from zero, and choose a specific salt.
        // base (no calldata):
        deploy_syscall(stable_class_hash, 1, array![0].span(), true).unwrap_syscall();
        // linear factor (calldata len = 1):
        deploy_syscall(stable_class_hash, 1, array![1, 0].span(), true).unwrap_syscall();

        // emit event syscall.
        emit_event_syscall(array![5].span(), array![7].span()).unwrap_syscall();
    }
}
