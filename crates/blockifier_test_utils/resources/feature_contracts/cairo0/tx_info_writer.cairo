%lang starknet

from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.starknet.common.syscalls import call_contract, get_tx_info

// This should match the value of the offset used in `test_deprecated_tx_info`.
const OFFSET = 0x1234;

@storage_var
func version(tx_type: felt) -> (version: felt) {
}

@storage_var
func account_contract_address(tx_type: felt) -> (account_contract_address: felt) {
}

@storage_var
func max_fee(tx_type: felt) -> (max_fee: felt) {
}

@storage_var
func signature_len(tx_type: felt) -> (signature_len: felt) {
}

@storage_var
func transaction_hash(tx_type: felt) -> (transaction_hash: felt) {
}

@storage_var
func chain_id(tx_type: felt) -> (chain_id: felt) {
}

@storage_var
func nonce(tx_type: felt) -> (nonce: felt) {
}

@external
func write{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    tx_type: felt, offset: felt
) {
    let (tx_info) = get_tx_info();

    // Add offset to values as writing '0' to the storage does not trigger a storage_update.
    version.write(tx_type, tx_info.version + offset);
    account_contract_address.write(tx_type, tx_info.account_contract_address + offset);
    max_fee.write(tx_type, tx_info.max_fee + offset);
    signature_len.write(tx_type, tx_info.signature_len + offset);
    transaction_hash.write(tx_type, tx_info.transaction_hash + offset);
    chain_id.write(tx_type, tx_info.chain_id + offset);
    nonce.write(tx_type, tx_info.nonce + offset);

    return ();
}

@l1_handler
func l1_write{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(from_address: felt) {
    write(tx_type='L1_HANDLER', offset=OFFSET);
    return ();
}

@external
func __validate__{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    call_write: felt
) {
    write(tx_type='INVOKE_FUNCTION', offset=OFFSET);
    return ();
}

@external
func __validate_declare__{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    class_hash: felt
) {
    write(tx_type='DECLARE', offset=OFFSET);
    return ();
}

@external
func __validate_deploy__{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    class_hash: felt, contract_address_salt: felt
) {
    write(tx_type='DEPLOY_ACCOUNT', offset=OFFSET);
    return ();
}

@external
func __execute__{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    call_write: felt
) {
    if (call_write != 0) {
        write(tx_type=0, offset=0);
        return ();
    }
    return ();
}
