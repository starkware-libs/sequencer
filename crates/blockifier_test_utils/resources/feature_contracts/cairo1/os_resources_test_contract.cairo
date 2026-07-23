// Contract for measuring per-syscall OS resource costs.
#[starknet::contract(account)]
mod OsResourcesTestContract {
    use box::BoxTrait;
    use core::sha256::{SHA256_INITIAL_STATE, sha256_state_handle_init};
    use core::sha512::compute_sha512_u64_array;
    use starknet::info::SyscallResultTrait;
    use starknet::secp256_trait::Secp256Trait;
    use starknet::secp256k1::{
        secp256k1_add_syscall, secp256k1_get_point_from_x_syscall, secp256k1_get_xy_syscall,
        secp256k1_mul_syscall, secp256k1_new_syscall,
    };
    use starknet::secp256r1::{
        secp256r1_add_syscall, secp256r1_get_point_from_x_syscall, secp256r1_get_xy_syscall,
        secp256r1_mul_syscall, secp256r1_new_syscall,
    };
    use starknet::syscalls::{
        call_contract_syscall, deploy_syscall, emit_event_syscall, get_execution_info_v3_syscall,
        keccak_syscall, library_call_syscall, replace_class_syscall, send_message_to_l1_syscall,
        sha256_process_block_syscall,
    };
    use starknet::{ClassHash, ContractAddress, get_block_hash_syscall, get_class_hash_at_syscall};

    const STABLE_EXTERNAL_ENTRY_POINT_SELECTOR: felt252 = selector!("external");
    const EXECUTE_FUNCTION_SELECTOR: felt252 = selector!("__execute__");

    // Define a large input length for variable-length input syscalls.
    const LARGE_INPUT_LENGTH: usize = 100;

    // SECP constants.
    const MULT_CONSTANT: u256 =
        115792089237316195423570985008687907853269984665640564039457584007913129639935;
    const X_FOR_K: u256 =
        111793196543967404139194827996419963236210979610743141064269745943111491389389;
    const Y_FOR_K: u256 =
        64271137072396112709852516195602121116634737731930508083758518861847052748305;
    const X_FOR_R: u256 =
        36259703446750261746963965979921905598426482711143882545997285073084044643087;
    const Y_FOR_R: u256 =
        99074502569356486940077471307887399820854676440660107539358273498981469249968;

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

        // Define a large input for variable-length input syscalls.
        let mut large_input: Array<felt252> = array![LARGE_INPUT_LENGTH.into()];
        for _ in 0..LARGE_INPUT_LENGTH {
            large_input.append(1);
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
            calldata: large_input.span(),
            signature: array![].span(),
        )
            .unwrap_syscall();

        // deploy syscall. The resources this syscall consumes can vary depending on the deployed
        // contract address, in a non-trivial way (see `normalize_address` in the cairo0 core). For
        // this reason we deploy from zero, and choose a specific salt.
        // base:
        deploy_syscall(stable_class_hash, 3, array![0].span(), true).unwrap_syscall();
        // linear factor:
        deploy_syscall(stable_class_hash, 3, large_input.span(), true).unwrap_syscall();

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
        for _ in 0..(17 * LARGE_INPUT_LENGTH) {
            input.append(1_u64);
        }
        keccak_syscall(input.span()).unwrap_syscall();

        // sha256.
        let mut input = BoxTrait::new([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        let mut state = sha256_state_handle_init(BoxTrait::new(SHA256_INITIAL_STATE));
        let _ = sha256_process_block_syscall(state, input).unwrap_syscall();

        // sha512. sha512_state_handle_init is pub(crate) in 2.19.0-rc.3, so call the public
        // high-level API which internally invokes sha512_process_block_syscall once.
        compute_sha512_u64_array(array![], 0, 0);

        // replace class syscall.
        replace_class_syscall(stable_class_hash).unwrap_syscall();

        // send message to l1 syscall.
        // No need to prepend the "calldata length" to the message payload.
        let mut large_message_input = large_input.clone();
        large_message_input.pop_front().unwrap();
        send_message_to_l1_syscall(100, array![].span()).unwrap_syscall();
        send_message_to_l1_syscall(100, large_message_input.span()).unwrap_syscall();

        // secp256k1 syscalls:

        // secp256k1_new syscall.
        let p0 = secp256k1_new_syscall(X_FOR_K, Y_FOR_K).unwrap_syscall().unwrap();

        //secp256k1_add syscall.
        let k_p1 = Secp256Trait::get_generator_point();
        secp256k1_add_syscall(p0, k_p1).unwrap_syscall();

        // secp256k1_get_point_from_x syscall.
        let x: u256 = X_FOR_K;
        let _ = secp256k1_get_point_from_x_syscall(:x, y_parity: true).unwrap_syscall();

        // secp256k1_get_xy syscall.
        secp256k1_get_xy_syscall(k_p1).unwrap_syscall();

        // secp256k1_mul syscall.
        secp256k1_mul_syscall(k_p1, MULT_CONSTANT).unwrap_syscall();

        // secp256r1 syscalls:

        // secp256r1_new syscall.
        let p0 = secp256r1_new_syscall(X_FOR_R, Y_FOR_R).unwrap_syscall().unwrap();
        let r_p1 = Secp256Trait::get_generator_point();

        // secp256r1_add syscall.
        secp256r1_add_syscall(p0, r_p1).unwrap_syscall();

        // secp256r1_get_point_from_x syscall.
        let x = X_FOR_R;
        let _ = secp256r1_get_point_from_x_syscall(:x, y_parity: true).unwrap_syscall();

        // secp256r1_get_xy syscall.
        secp256r1_get_xy_syscall(r_p1).unwrap_syscall();

        // secp256r1_mul syscall.
        secp256r1_mul_syscall(r_p1, MULT_CONSTANT).unwrap_syscall();
    }
}
