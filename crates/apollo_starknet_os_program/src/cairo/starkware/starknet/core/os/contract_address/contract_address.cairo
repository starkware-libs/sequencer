from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.hash_state import (
    hash_finalize,
    hash_init,
    hash_update_single,
    hash_update_with_hashchain,
)
from starkware.starknet.common.storage import normalize_address
from starkware.starknet.core.os.hash.hash_state_blake import HashState as BlakeHashState
from starkware.starknet.core.os.hash.hash_state_blake import hash_finalize as hash_finalize_blake
from starkware.starknet.core.os.hash.hash_state_blake import hash_init as hash_init_blake
from starkware.starknet.core.os.hash.hash_state_blake import (
    hash_update_single as hash_update_single_blake,
)
from starkware.starknet.core.os.hash.hash_state_blake import (
    hash_update_with_nested_hash as hash_update_with_nested_hash_blake,
)

const CONTRACT_ADDRESS_PREFIX = 'STARKNET_CONTRACT_ADDRESS';

func get_contract_address{hash_ptr: HashBuiltin*, range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_size: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=CONTRACT_ADDRESS_PREFIX
    );
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=deployer_address);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=salt);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=class_hash);
    let (hash_state_ptr) = hash_update_with_hashchain(
        hash_state_ptr=hash_state_ptr,
        data_ptr=constructor_calldata,
        data_length=constructor_calldata_size,
    );
    let (contract_address_before_modulo) = hash_finalize(hash_state_ptr=hash_state_ptr);
    let (contract_address) = normalize_address(addr=contract_address_before_modulo);

    return (contract_address=contract_address);
}

// Same as `get_contract_address`, but uses Blake2s (the optimized
// `encode_felt252_data_and_calc_blake_hash` encoding) instead of Pedersen.
func get_contract_address_blake{range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_size: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let hash_state: BlakeHashState = hash_init_blake();
    with hash_state {
        hash_update_single_blake(item=CONTRACT_ADDRESS_PREFIX);
        hash_update_single_blake(item=deployer_address);
        hash_update_single_blake(item=salt);
        hash_update_single_blake(item=class_hash);
        hash_update_with_nested_hash_blake(
            data_ptr=constructor_calldata, data_length=constructor_calldata_size
        );
    }
    let contract_address_before_modulo: felt = hash_finalize_blake(hash_state=hash_state);
    let (contract_address) = normalize_address(addr=contract_address_before_modulo);

    return (contract_address=contract_address);
}
