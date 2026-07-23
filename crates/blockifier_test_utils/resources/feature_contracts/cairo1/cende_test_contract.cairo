// Dedicated feature contract for the cende blob regression test
// (`central_systest_blobs::cende_blob_regression_test::test_make_data`).
//
// The cende test declares and deploys this contract, then exercises it to produce
// the block that is captured into `preconfirmed_block.json` and the blob stored in
// the `apollo-central-systest-blobs` GCS bucket. Using a dedicated contract here
// decouples those goldens from the shared `test_contract`: future changes to
// `test_contract` (the kitchen sink) do NOT churn cende anymore.
//
// New entry points needed by the cende test should be added HERE, not to
// `test_contract`. The functions below are minimal verbatim copies of the
// `test_contract` entry points that the cende test currently invokes.

#[starknet::contract]
mod CendeTestContract {
    use starknet::storage_access::{
        storage_address_from_base_and_offset, storage_base_address_from_felt252,
    };
    use starknet::{
        ContractAddress, StorageAddress, storage_read_syscall, storage_write_syscall, syscalls,
    };
    use starknet::SyscallResultTrait;

    #[storage]
    struct Storage {
        my_storage_var: felt252,
    }

    #[constructor]
    fn constructor(ref self: ContractState, arg1: felt252, arg2: felt252) -> felt252 {
        self.my_storage_var.write(arg1 + arg2);
        arg1
    }

    #[external(v0)]
    fn test_storage_write(ref self: ContractState, address: felt252, value: felt252) {
        let domain_address = 0_u32; // Only address_domain 0 is currently supported.
        let storage_address = storage_address_from_base_and_offset(
            storage_base_address_from_felt252(address), 0_u8,
        );
        storage_write_syscall(domain_address, storage_address, value).unwrap_syscall();
    }

    #[external(v0)]
    fn test_increment(
        ref self: ContractState, ref arg: felt252, arg1: felt252, arg2: felt252,
    ) -> felt252 {
        let x = self.my_storage_var.read();
        self.my_storage_var.write(x + 1);
        x + 1
    }

    #[external(v0)]
    fn test_storage_read_write(
        self: @ContractState, address: StorageAddress, value: felt252,
    ) -> felt252 {
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, address, value).unwrap_syscall();
        syscalls::storage_read_syscall(address_domain, address).unwrap_syscall()
    }

    #[external(v0)]
    fn write_and_revert(self: @ContractState, address: StorageAddress, value: felt252) {
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, address, value).unwrap_syscall();
        assert(1 == 0, 'Panic for revert');
    }

    #[external(v0)]
    #[raw_output]
    fn test_call_contract(
        self: @ContractState,
        contract_address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Array<felt252>,
    ) -> Span<felt252> {
        syscalls::call_contract_syscall(contract_address, entry_point_selector, calldata.span())
            .unwrap_syscall()
    }

    #[external(v0)]
    fn write_1(ref self: ContractState, key: StorageAddress) {
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, key, 1).unwrap_syscall();
        // Emit a dummy event and send a dummy L1 message (should be reverted by caller panic).
        let dummy_span = array![0].span();
        syscalls::emit_event_syscall(dummy_span, dummy_span).unwrap_syscall();
        syscalls::send_message_to_l1_syscall(17.try_into().unwrap(), dummy_span).unwrap_syscall();
    }

    // call_write_rewrite_panic: calls write_1, then writes storage cell to 2, then panics.
    #[external(v0)]
    fn call_write_rewrite_panic(
        ref self: ContractState, contract_address: ContractAddress, key: StorageAddress,
    ) {
        // Call write_1 which writes 1 to the storage cell.
        let calldata = array![key.into()];
        syscalls::call_contract_syscall(contract_address, selector!("write_1"), calldata.span())
            .unwrap_syscall();
        // Now write 2 to the same storage cell
        let address_domain = 0;
        syscalls::storage_write_syscall(address_domain, key, 2).unwrap_syscall();
        // Panic to trigger revert
        core::panic_with_felt252('call_write_rewrite_panic');
    }

    // catch_write_revert_panic: calls call_write_rewrite_panic and catches/ignores the revert,
    // then reads the storage cell to confirm the revert undid the inner writes.
    #[external(v0)]
    fn catch_write_revert_panic(
        ref self: ContractState, contract_address: ContractAddress, key: StorageAddress,
    ) -> felt252 {
        // Call call_write_rewrite_panic which will revert.
        let calldata = array![contract_address.into(), key.into()];
        match syscalls::call_contract_syscall(
            contract_address, selector!("call_write_rewrite_panic"), calldata.span(),
        ) {
            Result::Ok(_) => core::panic_with_felt252('expected_fail'),
            Result::Err(_) => {} // Ignore the revert
        }
        // Read the storage value - should be 0 (original) not 1 (write_1's write)
        let address_domain = 0;
        syscalls::storage_read_syscall(address_domain, key).unwrap_syscall()
    }
}
