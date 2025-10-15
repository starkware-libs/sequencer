%lang starknet

from starkware.starknet.common.syscalls import storage_write

@external
func test_storage_write{syscall_ptr: felt*, range_check_ptr}(address: felt, value: felt) {
    storage_write(address=address, value=value);

    return ();
}

@l1_handler
func test_l1_handler_storage_write{syscall_ptr: felt*, range_check_ptr}(
    from_address: felt, address: felt, value: felt
) {
    storage_write(address=address, value=value);

    return ();
}

@external
func foo() -> (res: felt) {
    return (res=234);
}
