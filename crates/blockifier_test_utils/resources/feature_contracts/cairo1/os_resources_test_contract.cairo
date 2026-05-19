// Contract for measuring per-syscall OS resource costs. Calls each measured syscall exactly once
// (secp256k1_new and secp256r1_new are called twice each explicitly).
// Uses get_contract_address() for self-referencing calls so the contract can be deployed at any
// address by the test infrastructure.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use array::ArrayTrait;
    use box::BoxTrait;
    use core::sha256::{SHA256_INITIAL_STATE, sha256_state_handle_init};
    use option::OptionTrait;
    use starknet::info::SyscallResultTrait;
    use starknet::secp256k1::{
        Secp256k1Point, secp256k1_add_syscall, secp256k1_get_point_from_x_syscall,
        secp256k1_get_xy_syscall, secp256k1_mul_syscall, secp256k1_new_syscall,
    };
    use starknet::secp256r1::{
        Secp256r1Point, secp256r1_add_syscall, secp256r1_get_point_from_x_syscall,
        secp256r1_get_xy_syscall, secp256r1_mul_syscall, secp256r1_new_syscall,
    };
    use starknet::storage_access::{
        storage_address_from_base, storage_base_address_from_felt252,
    };
    use starknet::syscalls::{
        call_contract_syscall, deploy_syscall, emit_event_syscall, get_class_hash_at_syscall,
        get_execution_info_v2_syscall, library_call_syscall, replace_class_syscall,
        send_message_to_l1_syscall, sha256_process_block_syscall, storage_read_syscall,
        storage_write_syscall,
    };
    use starknet::{ClassHash, ContractAddress};

    // Selector for the empty_function entry point of this contract.
    const EMPTY_FUNCTION_SELECTOR: felt252
        = 0x227AC0F3CE8083231605CB10BE915BE2004456B618E44B56067E27FC6F8C84F;

    // Large scalar used for secp256k1_mul and secp256r1_mul; kept < curve order for validity.
    const MULT_CONSTANT: u256
        = 115792089237316195423570985008687907853269984665640564039457584007913129639935;

    // A valid secp256k1 point (x, y).
    const X_FOR_K: u256
        = 111793196543967404139194827996419963236210979610743141064269745943111491389389;
    const Y_FOR_K: u256
        = 64271137072396112709852516195602121116634737731930508083758518861847052748305;

    // A valid secp256r1 point (x, y).
    const X_FOR_R: u256
        = 36259703446750261746963965979921905598426482711143882545997285073084044643087;
    const Y_FOR_R: u256
        = 99074502569356486940077471307887399820854676440660107539358273498981469249968;

    // An arbitrary L1 address used as the destination for send_message_to_l1.
    const L1_ADDRESS: felt252 = 42;

    #[storage]
    struct Storage {}

    #[constructor]
    fn constructor(ref self: ContractState) {}

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState, class_hash: ClassHash, contract_address_salt: felt252,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate_declare__(self: @ContractState, class_hash: ClassHash) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(self: @ContractState, self_class_hash: ClassHash) -> felt252 {
        starknet::VALIDATED
    }

    extern fn meta_tx_v0_syscall(
        address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Span<felt252>,
        signature: Span<felt252>,
    ) -> starknet::SyscallResult<Span<felt252>> implicits(GasBuiltin, System) nopanic;

    // Calls every measured syscall in order. Calldata: [self_class_hash, self_address].
    // The caller passes self_address explicitly to avoid an extra get_execution_info syscall
    // (which get_contract_address() would insert into the trace).
    #[external(v0)]
    fn __execute__(ref self: ContractState, self_class_hash: ClassHash, self_address: ContractAddress) {

        // call_contract syscall — calls empty_function on self.
        call_contract_syscall(
            self_address, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span(),
        )
            .unwrap_syscall();

        // library_call syscall — calls empty_function via class hash.
        library_call_syscall(
            self_class_hash, EMPTY_FUNCTION_SELECTOR, ArrayTrait::new().span(),
        )
            .unwrap_syscall();

        // meta_tx_v0 syscall — we only need it to appear in the OS trace for cost measurement;
        // success is not required since inner-call resources are not subtracted for this syscall.
        let _ = meta_tx_v0_syscall(
            address: self_address,
            entry_point_selector: EMPTY_FUNCTION_SELECTOR,
            calldata: ArrayTrait::new().span(),
            signature: ArrayTrait::new().span(),
        );

        // deploy syscall — deploys a new instance of this class with salt 0.
        deploy_syscall(self_class_hash, 0, ArrayTrait::new().span(), false).unwrap_syscall();

        // emit_event syscall.
        let mut keys = ArrayTrait::new();
        keys.append(5);
        let mut data = ArrayTrait::new();
        data.append(7);
        emit_event_syscall(keys.span(), data.span()).unwrap_syscall();

        // get_block_hash syscall — block 0 must be available (requires block_number >= 10).
        starknet::get_block_hash_syscall(0_u64).unwrap_syscall();

        // get_class_hash_at syscall.
        get_class_hash_at_syscall(self_address).unwrap_syscall();

        // get_execution_info syscall.
        get_execution_info_v2_syscall().unwrap_syscall();

        // sha256_process_block syscall.
        let sha256_input = BoxTrait::new(
            [1_u32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        );
        let sha256_state = sha256_state_handle_init(BoxTrait::new(SHA256_INITIAL_STATE));
        sha256_process_block_syscall(sha256_state, sha256_input).unwrap_syscall();

        // replace_class syscall.
        replace_class_syscall(self_class_hash).unwrap_syscall();

        // send_message_to_l1 syscall.
        let mut payload = ArrayTrait::new();
        payload.append(5);
        send_message_to_l1_syscall(L1_ADDRESS, payload.span()).unwrap_syscall();

        // secp256k1 syscalls — new called twice explicitly.
        let k_p0 = secp256k1_new_syscall(X_FOR_K, Y_FOR_K).unwrap_syscall().unwrap();
        let k_p1 = secp256k1_new_syscall(X_FOR_K, Y_FOR_K).unwrap_syscall().unwrap();
        secp256k1_add_syscall(k_p0, k_p1).unwrap_syscall();
        secp256k1_get_point_from_x_syscall(X_FOR_K, true).unwrap_syscall();
        secp256k1_get_xy_syscall(k_p0).unwrap_syscall();
        secp256k1_mul_syscall(k_p0, MULT_CONSTANT).unwrap_syscall();

        // secp256r1 syscalls — new called twice explicitly.
        let r_p0 = secp256r1_new_syscall(X_FOR_R, Y_FOR_R).unwrap_syscall().unwrap();
        let r_p1 = secp256r1_new_syscall(X_FOR_R, Y_FOR_R).unwrap_syscall().unwrap();
        secp256r1_add_syscall(r_p0, r_p1).unwrap_syscall();
        secp256r1_get_point_from_x_syscall(X_FOR_R, true).unwrap_syscall();
        secp256r1_get_xy_syscall(r_p0).unwrap_syscall();
        secp256r1_mul_syscall(r_p0, MULT_CONSTANT).unwrap_syscall();

        // storage_read syscall.
        let storage_address = storage_address_from_base(storage_base_address_from_felt252(0));
        storage_read_syscall(0_u32, storage_address).unwrap_syscall();

        // storage_write syscall.
        storage_write_syscall(0_u32, storage_address, 1991).unwrap_syscall();
    }

    // Target for call_contract, library_call, and meta_tx_v0 — accepts no arguments.
    #[external(v0)]
    fn empty_function(self: @ContractState) {}

    // Target for L1 handler tests.
    #[l1_handler]
    fn empty_l1_handler(self: @ContractState, from_address: felt252) {}
}
