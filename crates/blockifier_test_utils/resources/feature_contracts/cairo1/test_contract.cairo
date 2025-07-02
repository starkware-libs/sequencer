#[starknet::contract]
mod TestContract {
    use box::BoxTrait;
    use core::sha256::{compute_sha256_u32_array, sha256_state_handle_init, SHA256_INITIAL_STATE};
    use dict::Felt252DictTrait;
    use ec::EcPointTrait;
    use starknet::ClassHash;
    use starknet::ContractAddress;
    use starknet::get_execution_info;
    use starknet::StorageAddress;
    use array::ArrayTrait;
    use clone::Clone;
    use core::bytes_31::POW_2_128;
    use core::integer::bitwise;
    use traits::Into;
    use traits::TryInto;
    use starknet::{
        class_hash_try_from_felt252, contract_address_try_from_felt252,
        eth_address::U256IntoEthAddress, EthAddress, secp256_trait::{Signature, is_valid_signature},
        info::{BlockInfo, SyscallResultTrait}, info::v2::{ExecutionInfo, TxInfo, ResourceBounds,},
        secp256r1::{Secp256r1Point, Secp256r1Impl}, eth_signature::verify_eth_signature,
        storage_access::{storage_address_from_base_and_offset, storage_base_address_from_felt252},
        storage_write_syscall,
        syscalls
    };
    use core::circuit::{
        CircuitElement, CircuitInput, circuit_add, circuit_sub, circuit_mul, circuit_inverse,
        EvalCircuitResult, EvalCircuitTrait, u384, CircuitOutputsTrait, CircuitModulus,
        CircuitInputs, AddInputResultTrait
    };
    use core::hash::HashStateTrait;
    use core::pedersen::PedersenTrait;
    use core::poseidon::PoseidonTrait;

    #[storage]
    struct Storage {
        my_storage_var: felt252,
        revert_test_storage_var: felt252,
        two_counters: starknet::storage::Map<felt252, (felt252, felt252)>,
        ec_point: (felt252, felt252),
    }

    #[constructor]
    fn constructor(ref self: ContractState, arg1: felt252, arg2: felt252) -> felt252 {
        self.my_storage_var.write(arg1 + arg2);
        arg1
    }

    #[external(v0)]
    fn test(ref self: ContractState, ref arg: felt252, arg1: felt252, arg2: felt252) -> felt252 {
        let x = self.my_storage_var.read();
        self.my_storage_var.write(x + 1);
        x + 1
    }

    #[external(v0)]
    fn test_storage_write(ref self: ContractState, address: felt252, value: felt252) {
        let domain_address = 0_u32; // Only address_domain 0 is currently supported.
        let storage_address = storage_address_from_base_and_offset(
            storage_base_address_from_felt252(address), 0_u8
        );
        storage_write_syscall(domain_address, storage_address, value).unwrap_syscall();
    }

    #[external(v0)]
    fn test_storage_read_write(
        self: @ContractState, address: StorageAddress, value: felt252
    ) -> felt252 {
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, address, value).unwrap_syscall();
        syscalls::storage_read_syscall(address_domain, address).unwrap_syscall()
    }

    #[external(v0)]
    fn test_count_actual_storage_changes(self: @ContractState) {
        let storage_address = 15.try_into().unwrap();
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, storage_address, 0).unwrap_syscall();
        syscalls::storage_write_syscall(address_domain, storage_address, 1).unwrap_syscall();
    }

    #[external(v0)]
    #[raw_output]
    fn test_call_contract(
        self: @ContractState,
        contract_address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Array::<felt252>
    ) -> Span::<felt252> {
        syscalls::call_contract_syscall(contract_address, entry_point_selector, calldata.span())
            .unwrap_syscall()
    }


    #[external(v0)]
    fn call_execute_directly(
        ref self: ContractState, contract_address: ContractAddress, calldata: Array::<felt252>,
    ) -> Span::<felt252> {
        syscalls::call_contract_syscall(contract_address, selector!("__execute__"), calldata.span())
            .unwrap_syscall()
    }

    #[external(v0)]
    fn test_call_two_contracts(
        self: @ContractState,
        contract_address_0: ContractAddress,
        entry_point_selector_0: felt252,
        calldata_0: Array::<felt252>,
        contract_address_1: ContractAddress,
        entry_point_selector_1: felt252,
        calldata_1: Array::<felt252>,
    ) -> Span::<felt252> {
        let res_0 = syscalls::call_contract_syscall(
            contract_address_0, entry_point_selector_0, calldata_0.span(),
        )
            .unwrap_syscall();
        let res_1 = syscalls::call_contract_syscall(
            contract_address_1, entry_point_selector_1, calldata_1.span(),
        )
            .unwrap_syscall();
        let mut res: Array::<felt252> = Default::default();
        res.append_span(res_0);
        res.append_span(res_1);
        res.span()
    }

    #[external(v0)]
    fn test_revert_helper(
        ref self: ContractState, replacement_class_hash: ClassHash, to_panic: bool
    ) {
        let dummy_span = array![0].span();
        syscalls::emit_event_syscall(dummy_span, dummy_span).unwrap_syscall();
        syscalls::replace_class_syscall(replacement_class_hash).unwrap_syscall();
        syscalls::send_message_to_l1_syscall(17.try_into().unwrap(), dummy_span).unwrap_syscall();
        self.my_storage_var.write(17);
        if to_panic {
            panic(array!['test_revert_helper']);
        }
    }

    #[external(v0)]
    fn write_10_to_my_storage_var(ref self: ContractState) {
        self.my_storage_var.write(10);
    }

    /// Tests the behavior of a revert scenario with an inner contract call.
    /// The function performs the following:
    /// 1. Calls `write_10_to_my_storage_var` to set the storage variable to 10.
    /// 2. Calls `test_revert_helper` with `to_panic=true`.
    ///    - `test_revert_helper` is expected to change the storage variable to 17 and then panic.
    /// 3. Verifies that the `test_revert_helper` changes are reverted,
    /// ensuring the storage variable remains 10.
    #[external(v0)]
    fn test_revert_with_inner_call_and_reverted_storage(
        ref self: ContractState,
        contract_address: ContractAddress,
        replacement_class_hash: ClassHash,
    ) {
        // Step 1: Call the contract to set the storage variable to 10.
        syscalls::call_contract_syscall(
            contract_address, selector!("write_10_to_my_storage_var"), array![].span(),
        )
            .unwrap_syscall();

        // Step 2: Prepare the call to `test_revert_helper` with `to_panic = true`.
        let to_panic = true;
        let call_data = array![replacement_class_hash.into(), to_panic.into()];

        // Step 3: Call `test_revert_helper` and handle the expected panic.
        match syscalls::call_contract_syscall(
            contract_address, selector!("test_revert_helper"), call_data.span(),
        ) {
            Result::Ok(_) => panic(array!['should_panic']),
            Result::Err(_revert_reason) => {
                // Verify that the changes made by the second call are reverted.
                assert(self.my_storage_var.read() == 10, 'Wrong_storage_value.',);
            }
        }
    }

    #[external(v0)]
    fn middle_revert_contract(
        ref self: ContractState,
        contract_address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Array::<felt252>,
    ) {
        syscalls::call_contract_syscall(contract_address, entry_point_selector, calldata.span())
            .unwrap_syscall();
        panic(array!['execute_and_revert']);
    }


    #[external(v0)]
    fn test_emit_events(
        self: @ContractState, events_number: u64, keys: Array::<felt252>, data: Array::<felt252>
    ) {
        let mut c = 0_u64;
        loop {
            if c == events_number {
                break;
            }
            syscalls::emit_event_syscall(keys.span(), data.span()).unwrap_syscall();
            c += 1;
        };
    }

    #[external(v0)]
    fn test_get_class_hash_at(
        self: @ContractState, address: ContractAddress, expected_class_hash: ClassHash
    ) {
        let class_hash = syscalls::get_class_hash_at_syscall(address).unwrap_syscall();
        assert(class_hash == expected_class_hash, 'WRONG_CLASS_HASH');
    }

    #[external(v0)]
    fn test_get_block_hash(self: @ContractState, block_number: u64) -> felt252 {
        syscalls::get_block_hash_syscall(block_number).unwrap_syscall()
    }

    #[external(v0)]
    fn test_get_execution_info(
        self: @ContractState,
        expected_block_info: BlockInfo,
        expected_tx_info: TxInfo,
        // Expected call info.
        expected_caller_address: felt252,
        expected_contract_address: felt252,
        expected_entry_point_selector: felt252,
    ) {
        let execution_info = starknet::get_execution_info().unbox();
        let block_info = execution_info.block_info.unbox();
        assert(block_info == expected_block_info, 'BLOCK_INFO_MISMATCH');

        let tx_info = execution_info.tx_info.unbox();
        assert(tx_info == expected_tx_info, 'TX_INFO_MISMATCH');

        assert(execution_info.caller_address.into() == expected_caller_address, 'CALLER_MISMATCH');
        assert(
            execution_info.contract_address.into() == expected_contract_address, 'CONTRACT_MISMATCH'
        );
        assert(
            execution_info.entry_point_selector == expected_entry_point_selector,
            'SELECTOR_MISMATCH'
        );
    }

    #[external(v0)]
    fn test_get_execution_info_without_block_info(
        ref self: ContractState,
        // Expected transaction info.
        version: felt252,
        account_address: felt252,
        max_fee: felt252,
        resource_bounds: Span::<ResourceBounds>,
        // Expected call info.
        caller_address: felt252,
        contract_address: felt252,
        entry_point_selector: felt252,
    ) {
        let execution_info = starknet::get_execution_info().unbox();
        let tx_info = execution_info.tx_info.unbox();
        assert(tx_info.version == version, 'VERSION_MISMATCH');
        assert(tx_info.account_contract_address.into() == account_address, 'ACCOUNT_MISMATCH');
        assert(tx_info.max_fee.into() == max_fee, 'MAX_FEE_MISMATCH');
        assert(tx_info.resource_bounds == resource_bounds, 'RESOURCE_BOUND_MISMATCH');
        assert(tx_info.tip == 0_u128, 'TIP_MISMATCH');
        assert(tx_info.paymaster_data.len() == 0_u32, 'PAYMASTER_DATA_MISMATCH');
        assert(tx_info.nonce_data_availability_mode == 0_u32, 'NONCE_DA_MODE_MISMATCH');
        assert(tx_info.fee_data_availability_mode == 0_u32, 'FEE_DA_MODE_MISMATCH');
        assert(tx_info.account_deployment_data.len() == 0_u32, 'DEPLOYMENT_DATA_MISMATCH');
        assert(execution_info.caller_address.into() == caller_address, 'CALLER_MISMATCH');
        assert(execution_info.contract_address.into() == contract_address, 'CONTRACT_MISMATCH');
        assert(execution_info.entry_point_selector == entry_point_selector, 'SELECTOR_MISMATCH');
    }

    #[external(v0)]
    #[raw_output]
    fn test_library_call(
        self: @ContractState,
        class_hash: ClassHash,
        function_selector: felt252,
        calldata: Array<felt252>
    ) -> Span::<felt252> {
        starknet::library_call_syscall(class_hash, function_selector, calldata.span())
            .unwrap_syscall()
    }

    #[external(v0)]
    #[raw_output]
    fn test_nested_library_call(
        self: @ContractState,
        class_hash: ClassHash,
        lib_selector: felt252,
        nested_selector: felt252,
        a: felt252,
        b: felt252
    ) -> Span::<felt252> {
        let mut nested_library_calldata: Array::<felt252> = Default::default();
        nested_library_calldata.append(class_hash.into());
        nested_library_calldata.append(nested_selector);
        nested_library_calldata.append(2);
        nested_library_calldata.append(a + 1);
        nested_library_calldata.append(b + 1);
        let _res = starknet::library_call_syscall(
            class_hash, lib_selector, nested_library_calldata.span(),
        )
            .unwrap_syscall();

        let mut calldata: Array::<felt252> = Default::default();
        calldata.append(a);
        calldata.append(b);
        starknet::library_call_syscall(class_hash, nested_selector, calldata.span())
            .unwrap_syscall()
    }

    #[external(v0)]
    fn test_replace_class(self: @ContractState, class_hash: ClassHash) {
        syscalls::replace_class_syscall(class_hash).unwrap_syscall();
    }

    #[external(v0)]
    fn test_send_message_to_l1(
        self: @ContractState, to_address: felt252, payload: Array::<felt252>
    ) {
        starknet::send_message_to_l1_syscall(to_address, payload.span()).unwrap_syscall();
    }

    /// An external method that requires the `segment_arena` builtin.
    #[external(v0)]
    fn segment_arena_builtin(self: @ContractState) {
        let x = felt252_dict_new::<felt252>();
        x.squash();
    }

    #[l1_handler]
    fn l1_handle(self: @ContractState, from_address: felt252, arg: felt252) -> felt252 {
        arg
    }

    #[l1_handler]
    fn l1_handler_set_value(
        self: @ContractState, from_address: felt252, key: StorageAddress, value: felt252
    ) -> felt252 {
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, key, value).unwrap_syscall();
        value
    }

    #[external(v0)]
    fn test_deploy(
        self: @ContractState,
        class_hash: ClassHash,
        contract_address_salt: felt252,
        calldata: Array::<felt252>,
        deploy_from_zero: bool,
    ) {
        syscalls::deploy_syscall(
            class_hash, contract_address_salt, calldata.span(), deploy_from_zero
        )
            .unwrap_syscall();
    }

    #[external(v0)]
    fn test_stack_overflow(ref self: ContractState, depth: u128) -> u128 {
        non_trivial_recursion(depth)
    }

    fn non_trivial_recursion(depth: u128) -> u128 {
        non_trivial_recursion(depth - 1) + 2 * non_trivial_recursion(depth - 2)
    }

    #[external(v0)]
    fn test_keccak(ref self: ContractState) {
        let mut input: Array::<u256> = Default::default();
        input.append(u256 { low: 1, high: 0 });

        let res = keccak::keccak_u256s_le_inputs(input.span());
        assert(res.low == 0x587f7cc3722e9654ea3963d5fe8c0748, 'Wrong hash value');
        assert(res.high == 0xa5963aa610cb75ba273817bce5f8c48f, 'Wrong hash value');

        let mut input: Array::<u64> = Default::default();
        input.append(1_u64);
        match syscalls::keccak_syscall(input.span()) {
            Result::Ok(_) => panic_with_felt252('Should fail'),
            Result::Err(revert_reason) => assert(
                *revert_reason.at(0) == 'Invalid input length', 'Wrong error msg'
            ),
        }
    }

    #[external(v0)]
    fn test_sha256(ref self: ContractState) {
        let mut input: Array::<u32> = Default::default();
        input.append('aaaa');

        // Test the sha256 syscall computation of the string 'aaaa'.
        let [res, _, _, _, _, _, _, _,] = compute_sha256_u32_array(input, 0, 0);
        assert(res == 0x61be55a8, 'Wrong hash value');
    }

    #[external(v0)]
    fn test_secp256k1(ref self: ContractState) {
        // Test a point not on the curve.
        assert(
            starknet::secp256k1::secp256k1_new_syscall(x: 0, y: 1).unwrap_syscall().is_none(),
            'Should be none'
        );

        let secp256k1_prime = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f;
        match starknet::secp256k1::secp256k1_new_syscall(x: secp256k1_prime, y: 1) {
            Result::Ok(_) => panic_with_felt252('Should fail'),
            Result::Err(revert_reason) => assert(
                *revert_reason.at(0) == 'Invalid argument', 'Wrong error msg'
            ),
        }

        // Test a point on the curve.
        let x = 0xF728B4FA42485E3A0A5D2F346BAA9455E3E70682C2094CAC629F6FBED82C07CD;
        let y = 0x8E182CA967F38E1BD6A49583F43F187608E031AB54FC0C4A8F0DC94FAD0D0611;
        let p0 = starknet::secp256k1::secp256k1_new_syscall(x, y).unwrap_syscall().unwrap();

        let (x_coord, y_coord) = starknet::secp256k1::secp256k1_get_xy_syscall(p0).unwrap_syscall();
        assert(x_coord == x && y_coord == y, 'Unexpected coordinates');

        let (msg_hash, signature, _expected_public_key_x, _expected_public_key_y, eth_address) =
            get_message_and_secp256k1_signature();
        verify_eth_signature(:msg_hash, :signature, :eth_address);
    }

    #[external(v0)]
    fn test_secp256k1_point_from_x(ref self: ContractState) { // Test a point not on the curve.
        assert(
            starknet::secp256k1::secp256k1_get_point_from_x_syscall(x: 0, y_parity: true)
                .unwrap_syscall()
                .is_none(),
            'Should be none'
        );

        //Test a point on the curve.
        let x = 0xF728B4FA42485E3A0A5D2F346BAA9455E3E70682C2094CAC629F6FBED82C07CD;
        let p0 = starknet::secp256k1::secp256k1_get_point_from_x_syscall(x: x, y_parity: true)
            .unwrap_syscall()
            .unwrap();
        let p1 = starknet::secp256k1::secp256k1_get_point_from_x_syscall(x: x, y_parity: false)
            .unwrap_syscall()
            .unwrap();

        let expected_y = 0x8E182CA967F38E1BD6A49583F43F187608E031AB54FC0C4A8F0DC94FAD0D0611;
        let (x_coord, y_coord) = starknet::secp256k1::secp256k1_get_xy_syscall(p0).unwrap_syscall();
        assert(x_coord == x && y_coord == expected_y, 'Unexpected coordinates');
        let secp256k1_prime = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f;
        let expected_p1_y = secp256k1_prime - expected_y;
        let (x_coord, y_coord) = starknet::secp256k1::secp256k1_get_xy_syscall(p1).unwrap_syscall();
        assert(x_coord == x && y_coord == expected_p1_y, 'Unexpected coordinates');
    }

    /// Returns a golden valid message hash and its signature, for testing.
    fn get_message_and_secp256k1_signature() -> (u256, Signature, u256, u256, EthAddress) {
        let msg_hash = 0xe888fbb4cf9ae6254f19ba12e6d9af54788f195a6f509ca3e934f78d7a71dd85;
        let r = 0x4c8e4fbc1fbb1dece52185e532812c4f7a5f81cf3ee10044320a0d03b62d3e9a;
        let s = 0x4ac5e5c0c0e8a4871583cc131f35fb49c2b7f60e6a8b84965830658f08f7410c;

        let (public_key_x, public_key_y) = (
            0xa9a02d48081294b9bb0d8740d70d3607feb20876964d432846d9b9100b91eefd,
            0x18b410b5523a1431024a6ab766c89fa5d062744c75e49efb9925bf8025a7c09e
        );

        let eth_address = 0x767410c1bb448978bd42b984d7de5970bcaf5c43_u256.into();

        (msg_hash, Signature { r, s, y_parity: true }, public_key_x, public_key_y, eth_address)
    }


    #[external(v0)]
    fn test_secp256r1(ref self: ContractState) {
        // Test a point not on the curve.
        assert(
            starknet::secp256r1::secp256r1_new_syscall(x: 0, y: 1).unwrap_syscall().is_none(),
            'Should be none'
        );

        let secp256r1_prime = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff;
        match starknet::secp256r1::secp256r1_new_syscall(x: secp256r1_prime, y: 1) {
            Result::Ok(_) => panic_with_felt252('Should fail'),
            Result::Err(revert_reason) => assert(
                *revert_reason.at(0) == 'Invalid argument', 'Wrong error msg'
            ),
        }

        // Test a point on the curve.
        let x = 0x502A43CE77C6F5C736A82F847FA95F8C2D483FE223B12B91047D83258A958B0F;
        let y = 0xDB0A2E6710C71BA80AFEB3ABDF69D306CE729C7704F4DDF2EAAF0B76209FE1B0;
        let p0 = starknet::secp256r1::secp256r1_new_syscall(x, y).unwrap_syscall().unwrap();

        let (x_coord, y_coord) = starknet::secp256r1::secp256r1_get_xy_syscall(p0).unwrap_syscall();
        assert(x_coord == x && y_coord == y, 'Unexpected coordinates');

        let (msg_hash, signature, expected_public_key_x, expected_public_key_y, _eth_address) =
            get_message_and_secp256r1_signature();
        let public_key = Secp256r1Impl::secp256_ec_new_syscall(
            expected_public_key_x, expected_public_key_y
        )
            .unwrap_syscall()
            .unwrap();
        is_valid_signature::<Secp256r1Point>(msg_hash, signature.r, signature.s, public_key);
    }


    /// Returns a golden valid message hash and its signature, for testing.
    fn get_message_and_secp256r1_signature() -> (u256, Signature, u256, u256, EthAddress) {
        let msg_hash = 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855;
        let r = 0xb292a619339f6e567a305c951c0dcbcc42d16e47f219f9e98e76e09d8770b34a;
        let s = 0x177e60492c5a8242f76f07bfe3661bde59ec2a17ce5bd2dab2abebdf89a62e2;

        let (public_key_x, public_key_y) = (
            0x04aaec73635726f213fb8a9e64da3b8632e41495a944d0045b522eba7240fad5,
            0x0087d9315798aaa3a5ba01775787ced05eaaf7b4e09fc81d6d1aa546e8365d525d
        );
        let eth_address = 0x492882426e1cda979008bfaf874ff796eb3bb1c0_u256.into();

        (msg_hash, Signature { r, s, y_parity: true }, public_key_x, public_key_y, eth_address)
    }

    impl ResourceBoundsPartialEq of PartialEq<ResourceBounds> {
        #[inline(always)]
        fn eq(lhs: @ResourceBounds, rhs: @ResourceBounds) -> bool {
            (*lhs.resource == *rhs.resource)
                && (*lhs.max_amount == *rhs.max_amount)
                && (*lhs.max_price_per_unit == *rhs.max_price_per_unit)
        }
        #[inline(always)]
        fn ne(lhs: @ResourceBounds, rhs: @ResourceBounds) -> bool {
            !(*lhs == *rhs)
        }
    }

    impl TxInfoPartialEq of PartialEq<TxInfo> {
        #[inline(always)]
        fn eq(lhs: @TxInfo, rhs: @TxInfo) -> bool {
            (*lhs.version == *rhs.version)
                && (*lhs.account_contract_address == *rhs.account_contract_address)
                && (*lhs.max_fee == *rhs.max_fee)
                && (*lhs.signature == *rhs.signature)
                && (*lhs.transaction_hash == *rhs.transaction_hash)
                && (*lhs.chain_id == *rhs.chain_id)
                && (*lhs.nonce == *rhs.nonce)
                && (*lhs.resource_bounds == *rhs.resource_bounds)
                && (*lhs.tip == *rhs.tip)
                && (*lhs.paymaster_data == *rhs.paymaster_data)
                && (*lhs.nonce_data_availability_mode == *rhs.nonce_data_availability_mode)
                && (*lhs.fee_data_availability_mode == *rhs.fee_data_availability_mode)
                && (*lhs.account_deployment_data == *rhs.account_deployment_data)
        }
        #[inline(always)]
        fn ne(lhs: @TxInfo, rhs: @TxInfo) -> bool {
            !(*lhs == *rhs)
        }
    }

    impl BlockInfoPartialEq of PartialEq<BlockInfo> {
        #[inline(always)]
        fn eq(lhs: @BlockInfo, rhs: @BlockInfo) -> bool {
            (*lhs.block_number == *rhs.block_number)
                && (*lhs.block_timestamp == *rhs.block_timestamp)
                && (*lhs.sequencer_address == *rhs.sequencer_address)
        }
        #[inline(always)]
        fn ne(lhs: @BlockInfo, rhs: @BlockInfo) -> bool {
            !(*lhs == *rhs)
        }
    }

    #[external(v0)]
    fn assert_eq(ref self: ContractState, x: felt252, y: felt252) -> felt252 {
        assert(x == y, 'x != y');
        'success'
    }

    #[external(v0)]
    fn invoke_call_chain(ref self: ContractState, mut call_chain: Array::<felt252>,) -> felt252 {
        // If the chain is too short, fail with division by zero.
        let len = call_chain.len();
        if len < 3 {
            return (1_u8 / 0_u8).into();
        }

        // Pop the parameters for the next call in the chain.
        let contract_id = call_chain.pop_front().unwrap();
        let function_selector = call_chain.pop_front().unwrap();
        let call_type = call_chain.pop_front().unwrap();

        // Choose call type according to the following options:
        // 0 - call contract syscall. 1 - library call syscall. other - regular inner call.
        // The remaining items of the call_chain array are passed on as calldata.
        if call_type == 0 {
            let contract_address = contract_address_try_from_felt252(contract_id).unwrap();
            syscalls::call_contract_syscall(contract_address, function_selector, call_chain.span())
                .unwrap_syscall();
        } else if call_type == 1 {
            let class_hash = class_hash_try_from_felt252(contract_id).unwrap();
            syscalls::library_call_syscall(class_hash, function_selector, call_chain.span())
                .unwrap_syscall();
        } else {
            let invoke_call_chain_selector: felt252 =
                0x0062c83572d28cb834a3de3c1e94977a4191469a4a8c26d1d7bc55305e640ed5;
            let fail_selector: felt252 =
                0x032564d7e0fe091d49b4c20f4632191e4ed6986bf993849879abfef9465def25;
            if function_selector == invoke_call_chain_selector {
                return invoke_call_chain(ref self, call_chain);
            }
            if function_selector == fail_selector {
                fail(ref self);
            }
        }
        return 0;
    }

    #[external(v0)]
    fn fail(ref self: ContractState) {
        panic_with_felt252('fail');
    }

    #[external(v0)]
    fn recursive_fail(ref self: ContractState, depth: felt252) {
        if depth == 0 {
            panic_with_felt252('recursive_fail');
        }
        recursive_fail(ref self, depth - 1)
    }

    #[external(v0)]
    fn recurse(ref self: ContractState, depth: felt252) {
        if depth == 0 {
            return;
        }
        recurse(ref self, depth - 1)
    }

    #[external(v0)]
    fn recursive_syscall(
        ref self: ContractState,
        contract_address: ContractAddress,
        function_selector: felt252,
        depth: felt252,
    ) {
        if depth == 0 {
            return;
        }
        let calldata: Array::<felt252> = array![
            contract_address.into(), function_selector, depth - 1
        ];
        syscalls::call_contract_syscall(contract_address, function_selector, calldata.span())
            .unwrap_syscall();
        return;
    }

    #[derive(Drop, Serde)]
    struct IndexAndValues {
        index: felt252,
        values: (u128, u128),
    }

    #[starknet::interface]
    trait MyContract<TContractState> {
        fn xor_counters(ref self: TContractState, index_and_x: IndexAndValues);
    }

    // Advances the 'two_counters' storage variable by 'diff'.
    #[external(v0)]
    fn advance_counter(ref self: ContractState, index: felt252, diff_0: felt252, diff_1: felt252) {
        let val = self.two_counters.read(index);
        let (val_0, val_1) = val;
        self.two_counters.write(index, (val_0 + diff_0, val_1 + diff_1));
    }

    #[external(v0)]
    fn xor_counters(ref self: ContractState, index_and_x: IndexAndValues) {
        let index = index_and_x.index;
        let (val_0, val_1) = index_and_x.values;
        let counters = self.two_counters.read(index);
        let (counter_0, counter_1) = counters;
        let counter_0: u128 = counter_0.try_into().unwrap();
        let counter_1: u128 = counter_1.try_into().unwrap();
        let res_0: felt252 = (counter_0 ^ val_0).into();
        let res_1: felt252 = (counter_1 ^ val_1).into();
        self.two_counters.write(index, (res_0, res_1));
    }

    #[external(v0)]
    fn call_xor_counters(
        ref self: ContractState, address: ContractAddress, index_and_x: IndexAndValues
    ) {
        MyContractDispatcher { contract_address: address }.xor_counters(index_and_x);
    }

    #[external(v0)]
    fn test_ec_op(ref self: ContractState) {
        let p = EcPointTrait::new(
            0x654fd7e67a123dd13868093b3b7777f1ffef596c2e324f25ceaf9146698482c,
            0x4fad269cbf860980e38768fe9cb6b0b9ab03ee3fe84cfde2eccce597c874fd8
        )
            .unwrap();
        let q = EcPointTrait::new(
            0x3dbce56de34e1cfe252ead5a1f14fd261d520d343ff6b7652174e62976ef44d,
            0x4b5810004d9272776dec83ecc20c19353453b956e594188890b48467cb53c19
        )
            .unwrap();
        let m: felt252 = 0x6d232c016ef1b12aec4b7f88cc0b3ab662be3b7dd7adbce5209fcfdbd42a504;
        let res = q.mul(m) + p;
        let res_nz = res.try_into().unwrap();
        self.ec_point.write(res_nz.coordinates());
    }

    #[external(v0)]
    fn add_signature_to_counters(ref self: ContractState, index: felt252) {
        let signature = get_execution_info().unbox().tx_info.unbox().signature;
        let val = self.two_counters.read(index);
        let (val_0, val_1) = val;
        self.two_counters.write(index, (val_0 + *signature.at(0), val_1 + *signature.at(1)));
    }

    #[external(v0)]
    fn send_message(self: @ContractState, to_address: felt252) {
        let mut payload = ArrayTrait::<felt252>::new();
        payload.append(12);
        payload.append(34);
        starknet::send_message_to_l1_syscall(to_address, payload.span()).unwrap_syscall();
    }

    #[external(v0)]
    fn test_circuit(ref self: ContractState) {
        let in1 = CircuitElement::<CircuitInput<0>> {};
        let in2 = CircuitElement::<CircuitInput<1>> {};
        let add = circuit_add(in1, in2);
        let inv = circuit_inverse(add);
        let sub = circuit_sub(inv, in2);
        let mul = circuit_mul(inv, sub);

        let modulus = TryInto::<_, CircuitModulus>::try_into([7, 0, 0, 0]).unwrap();
        let outputs = (mul,)
            .new_inputs()
            .next([3, 0, 0, 0])
            .next([6, 0, 0, 0])
            .done()
            .eval(modulus)
            .unwrap();

        assert!(outputs.get_output(mul) == u384 { limb0: 6, limb1: 0, limb2: 0, limb3: 0 });
    }

    // Add drop for these objects as they only have PanicDestruct.
    impl AddInputResultDrop<C> of Drop<core::circuit::AddInputResult<C>>;
    impl CircuitDataDrop<C> of Drop<core::circuit::CircuitData<C>>;
    impl CircuitInputAccumulatorDrop<C> of Drop<core::circuit::CircuitInputAccumulator<C>>;

    #[external(v0)]
    fn test_rc96_holes(ref self: ContractState) {
        test_rc96_holes_helper();
        test_rc96_holes_helper();
    }

    #[inline(never)]
    fn test_rc96_holes_helper() {
        let in1 = CircuitElement::<CircuitInput<0>> {};
        (in1,).new_inputs().next([3, 0, 0, 0]);
    }

    #[external(v0)]
    fn test_call_contract_revert(
        ref self: ContractState,
        contract_address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Array::<felt252>
    ) {
        let class_hash_before_call = syscalls::get_class_hash_at_syscall(contract_address)
            .unwrap_syscall();
        self.revert_test_storage_var.write(7);
        match syscalls::call_contract_syscall(
            contract_address, entry_point_selector, calldata.span()
        ) {
            Result::Ok(_) => panic!("Expected revert"),
            Result::Err(errors) => {
                let mut error_span = errors.span();
                assert(*error_span.pop_back().unwrap() == 'ENTRYPOINT_FAILED', 'Unexpected error',);
                let inner_error = *error_span.pop_back().unwrap();
                if entry_point_selector == selector!("bad_selector") {
                    assert(inner_error == 'ENTRYPOINT_NOT_FOUND', 'Unexpected error');
                } else if entry_point_selector == selector!("test_revert_helper") {
                    assert(inner_error == 'test_revert_helper', 'Unexpected error');
                } else {
                    assert(
                        entry_point_selector == selector!("middle_revert_contract"),
                        'Wrong Entry Point'
                    );
                    assert(inner_error == 'execute_and_revert', 'Wrong_error');
                }
            },
        };
        let class_hash_after_call = syscalls::get_class_hash_at_syscall(contract_address)
            .unwrap_syscall();
        assert(self.my_storage_var.read() == 0, 'values should not change.');
        assert(class_hash_before_call == class_hash_after_call, 'class hash should not change.');
        assert(self.revert_test_storage_var.read() == 7, 'test_storage_var_changed.');
    }

    #[external(v0)]
    fn return_result(ref self: ContractState, num: felt252) -> felt252 {
        let result = num;
        result
    }

    #[external(v0)]
    fn empty_function(ref self: ContractState) {}

    #[external(v0)]
    fn test_bitwise(ref self: ContractState) {
        let x: u32 = 0x1;
        let y: u32 = 0x2;
        let _z = x & y;
    }

    #[external(v0)]
    fn test_pedersen(ref self: ContractState) {
        let mut state = PedersenTrait::new(0);
        state = state.update(1);
        let _hash = state.finalize();
    }

    #[external(v0)]
    fn test_poseidon(ref self: ContractState) {
        let mut state = PoseidonTrait::new();
        state = state.update(1);
        let _hash = state.finalize();
    }

    #[external(v0)]
    fn test_ecop(ref self: ContractState) {
        let m: felt252 = 2;
        let a: felt252 =
            336742005567258698661916498343089167447076063081786685068305785816009957563;
        let b: felt252 =
            1706004133033694959518200210163451614294041810778629639790706933324248611779;
        let p: ec::NonZeroEcPoint = (ec::ec_point_try_new_nz(a, b)).unwrap();
        let mut s: ec::EcState = ec::ec_state_init();
        ec::ec_state_add_mul(ref s, m, p);
    }
}

