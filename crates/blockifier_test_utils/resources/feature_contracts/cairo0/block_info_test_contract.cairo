%lang starknet
from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.math import assert_not_equal, unsigned_div_rem
from starkware.starknet.common.syscalls import get_block_number, get_block_timestamp

func test_block_info{syscall_ptr: felt*, range_check_ptr}(is_validate: felt) {
    alloc_locals;
    let (block_number) = get_block_number();
    let (block_timestamp) = get_block_timestamp();
    local syscall_ptr: felt* = syscall_ptr;
    test_given_block_info(
        block_number=block_number, block_timestamp=block_timestamp, is_validate=is_validate
    );
    return ();
}

func test_given_block_info{range_check_ptr}(
    block_number: felt, block_timestamp: felt, is_validate: felt
) {
    let (divided_block_number, _) = unsigned_div_rem(block_number, 100);
    tempvar block_number_for_validate = divided_block_number * 100;
    let (divided_block_timestamp, _) = unsigned_div_rem(block_timestamp, 3600);
    tempvar block_timestamp_for_validate = divided_block_timestamp * 3600;

    if (is_validate == TRUE) {
        assert block_number = block_number_for_validate;
        assert block_timestamp = block_timestamp_for_validate;
        return ();
    }

    assert is_validate = FALSE;
    // We assume that the block info members are not rounded in the test.
    assert_not_equal(block_number, block_number_for_validate);
    assert_not_equal(block_timestamp, block_timestamp_for_validate);
    return ();
}

@external
func __validate_declare__{syscall_ptr: felt*, range_check_ptr}(class_hash: felt) {
    test_block_info(is_validate=TRUE);
    return ();
}

@external
func __validate_deploy__{syscall_ptr: felt*, range_check_ptr}(
    class_hash: felt, contract_address_salt: felt, is_validate: felt
) {
    test_block_info(is_validate=TRUE);
    return ();
}

@external
func __validate__{syscall_ptr: felt*, range_check_ptr}(
    contract_address: felt, selector: felt, calldata_len: felt, calldata: felt*
) {
    test_block_info(is_validate=TRUE);
    return ();
}

@external
func __execute__{syscall_ptr: felt*, range_check_ptr}(
    contract_address, selector: felt, calldata_len: felt, calldata: felt*
) {
    test_block_info(is_validate=FALSE);
    return ();
}

@constructor
func constructor{syscall_ptr: felt*, range_check_ptr}(is_validate: felt) {
    test_block_info(is_validate=is_validate);
    return ();
}
