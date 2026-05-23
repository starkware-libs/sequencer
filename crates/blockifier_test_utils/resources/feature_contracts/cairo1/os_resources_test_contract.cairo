// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use box::BoxTrait;
    use core::sha256::{SHA256_INITIAL_STATE, sha256_state_handle_init};
    use starknet::class_hash::ClassHashZero;
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{
        call_contract_syscall, deploy_syscall, emit_event_syscall, get_execution_info_v2_syscall,
        keccak_syscall, library_call_syscall, replace_class_syscall, send_message_to_l1_syscall,
        sha256_process_block_syscall,
    };
    use starknet::{ClassHash, ContractAddress, get_block_hash_syscall, get_class_hash_at_syscall};

    const EMPTY_FUNCTION_SELECTOR: felt252 = selector!("empty_function");
    const EXECUTE_FUNCTION_SELECTOR: felt252 = selector!("__execute__");

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
        extra_args: Span<felt252>,
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
        ref self: ContractState,
        self_class_hash: ClassHash,
        self_address: ContractAddress,
        deployable_class_hash: ClassHash,
        extra_args: Span<felt252>,
    ) {
        // If called from the meta-tx-v0 syscall, just return.
        if self_class_hash == ClassHashZero::zero() {
            return;
        }

        // call_contract syscall — calls empty_function on self.
        call_contract_syscall(self_address, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span())
            .unwrap_syscall();

        // library_call syscall — calls empty_function on self.
        library_call_syscall(self_class_hash, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span())
            .unwrap_syscall();

        // meta_tx_v0 syscall - base.
        meta_tx_v0_syscall(
            address: self_address,
            entry_point_selector: EXECUTE_FUNCTION_SELECTOR,
            // class hash, address, deployable class hash, extra args len.
            calldata: array![0, 0, 0, 0].span(),
            signature: array![].span(),
        )
            .unwrap_syscall();
        // meta_tx_v0 syscall - linear factor.
        meta_tx_v0_syscall(
            address: self_address,
            entry_point_selector: EXECUTE_FUNCTION_SELECTOR,
            // class hash, address, deployable class hash, extra args len, extra arg.
            calldata: array![0, 0, 0, 1, 0].span(),
            signature: array![].span(),
        )
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

        // emit event syscall.
        emit_event_syscall(array![5].span(), array![7].span()).unwrap_syscall();

        // get block hash syscall.
        get_block_hash_syscall(0_u64).unwrap_syscall();

        // get class hash at syscall.
        get_class_hash_at_syscall(self_address).unwrap_syscall();

        // get execution info syscall.
        get_execution_info_v2_syscall().unwrap_syscall();

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

        // replace class syscall.
        replace_class_syscall(self_class_hash).unwrap_syscall();

        // send message to l1 syscall.
        // TODO(Yoni, 1/6/2022): In this case the number of steps depends on the payload size -
        //  consider counting it.
        send_message_to_l1_syscall(100, array![].span()).unwrap_syscall();
    }

    // Target for call_contract and library_call — accepts no arguments.
    #[external(v0)]
    fn empty_function(self: @ContractState) {}
}
