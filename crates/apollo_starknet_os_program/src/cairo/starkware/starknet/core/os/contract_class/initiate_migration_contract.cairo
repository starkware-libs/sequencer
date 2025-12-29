#[starknet::contract]
mod MigrationContract {
    use array::SpanTrait;
    use starknet::{
        panic_with,
        class_hash::ClassHash,
        syscalls::{library_call_syscall, SyscallError},
    };

    #[external(v0)]
    fn initiate_migration_library_call(
        ref self: ContractState,
        class_hash: ClassHash,
        entry_point_selector: felt252,
        calldata: Array::<felt252>,
    ) {
        let res = library_call_syscall(class_hash, entry_point_selector, calldata.span());
        match res {
            Result::Err(SyscallError::EntryPointNotFound) => { /* success */ }
            _ => panic_with("Expected EntryPointNotFound".into()),
        }
    }
}