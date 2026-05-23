// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use box::BoxTrait;
    use core::sha256::{SHA256_INITIAL_STATE, sha256_state_handle_init};
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{
        call_contract_syscall, deploy_syscall, emit_event_syscall, get_execution_info_v3_syscall,
        keccak_syscall, library_call_syscall, sha256_process_block_syscall,
    };
    use starknet::{ClassHash, ContractAddress, get_block_hash_syscall, get_class_hash_at_syscall};

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

        // get block hash syscall.
        // Only block numbers between `CURRENT_BLOCK_NUMBER - BLOCK_HASH_HISTORY_RANGE` and
        // `CURRENT_BLOCK_NUMBER - 10` are set in storage during OS flow tests; at the time of
        // writing this contract, `CURRENT_BLOCK_NUMBER` is 2001 and `BLOCK_HASH_HISTORY_RANGE` is
        // 51.
        get_block_hash_syscall(1970_u64).unwrap_syscall();

        // get class hash at syscall.
        get_class_hash_at_syscall(stable_address).unwrap_syscall();

        // get execution info syscall.
        get_execution_info_v3_syscall().unwrap_syscall();

        // keccak syscall. Second call is to measure the keccak round syscall.
        keccak_syscall(array![].span()).unwrap_syscall();
        // Exactly 17 input u64s are required to measure a single round.
        let mut input = array![];
        for _ in 0_u8..17 {
            input.append(1_u64);
        }
        keccak_syscall(input.span()).unwrap_syscall();

        // sha256.
        let mut input = BoxTrait::new([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        let mut state = sha256_state_handle_init(BoxTrait::new(SHA256_INITIAL_STATE));
        let _ = sha256_process_block_syscall(state, input).unwrap_syscall();
    }
}
