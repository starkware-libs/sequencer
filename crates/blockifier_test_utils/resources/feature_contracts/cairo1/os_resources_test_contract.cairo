// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use starknet::class_hash::ClassHashZero;
    use starknet::info::SyscallResultTrait;
    use starknet::syscalls::{
        call_contract_syscall,
        deploy_syscall,
        emit_event_syscall,
        get_execution_info_v2_syscall,
        library_call_syscall,
    };
    use starknet::{ClassHash, ContractAddress, get_block_hash_syscall, get_class_hash_at_syscall};

    const EMPTY_FUNCTION_SELECTOR: felt252 = selector!("empty_function");
    const EXECUTE_FUNCTION_SELECTOR: felt252 = selector!("__execute__");

    #[storage]
    struct Storage {}

    #[constructor]
    fn constructor(ref self: ContractState, some_args: Span<felt252>) {}

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
            // class hash, address, extra args len.
            calldata: array![0, 0, 0].span(),
            signature: array![].span(),
        ).unwrap_syscall();
        // meta_tx_v0 syscall - linear factor.
        meta_tx_v0_syscall(
            address: self_address,
            entry_point_selector: EXECUTE_FUNCTION_SELECTOR,
            // class hash, address, extra args len, extra arg.
            calldata: array![0, 0, 1, 0].span(),
            signature: array![].span(),
        ).unwrap_syscall();

        // deploy syscall: base (no calldata).
        deploy_syscall(self_class_hash, 0, array![0].span(), false).unwrap_syscall();
        // deploy syscall: linear factor (calldata len = 1).
        deploy_syscall(self_class_hash, 0, array![1, 0].span(), false).unwrap_syscall();

        // emit event syscall.
        emit_event_syscall(array![5].span(), array![7].span()).unwrap_syscall();

        // get block hash syscall.
        get_block_hash_syscall(0_u64).unwrap_syscall();

        // get class hash at syscall.
        get_class_hash_at_syscall(self_address).unwrap_syscall();

        // get execution info syscall.
        get_execution_info_v2_syscall().unwrap_syscall();
    }

    // Target for call_contract and library_call — accepts no arguments.
    #[external(v0)]
    fn empty_function(self: @ContractState) {}
}
