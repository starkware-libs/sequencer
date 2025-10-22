from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.hash_state import (
    hash_finalize,
    hash_init,
    hash_update,
    hash_update_single,
)
from starkware.cairo.common.registers import get_fp_and_pc

const STARKNET_OS_CONFIG_VERSION = 'StarknetOsConfig3';

const DEFAULT_PUBLIC_KEYS_HASH = 0;

struct StarknetOsConfig {
    // The identifier of the chain.
    // This field can be used to prevent replay of testnet transactions on mainnet.
    chain_id: felt,
    // The (L2) address of the fee token contract.
    fee_token_address: felt,
    // The hash of the public keys used to encrypt the state diff.
    // The default hash is 0, indicating that encryption will not happen and that there are no
    // public keys, as is the case in Starknet environments.
    public_keys_hash: felt,
}

// Calculates the hash of StarkNet OS config. The public keys hash is not included if there are no
// public keys (i.e., for envs where the state diff is not encrypted).
func get_starknet_os_config_hash{hash_ptr: HashBuiltin*}(starknet_os_config: StarknetOsConfig*) -> (
    starknet_os_config_hash: felt
) {
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=STARKNET_OS_CONFIG_VERSION
    );
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=starknet_os_config.chain_id
    );
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=starknet_os_config.fee_token_address
    );
    static_assert StarknetOsConfig.SIZE == 3;
    // If in the future another optional field is added to StarknetOsConfig,
    // remove the following `if`.
    if (starknet_os_config.public_keys_hash != DEFAULT_PUBLIC_KEYS_HASH) {
        let (hash_state_ptr) = hash_update_single(
            hash_state_ptr=hash_state_ptr, item=starknet_os_config.public_keys_hash
        );
    } else {
        // align the stack.
        tempvar hash_ptr = hash_ptr;
        tempvar hash_state_ptr = hash_state_ptr;
    }
    let (starknet_os_config_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);

    return (starknet_os_config_hash=starknet_os_config_hash);
}

// Computes the hash of the public keys, returns 0 if there are no public keys.
func get_public_keys_hash{hash_ptr: HashBuiltin*}(n_public_keys: felt, public_keys: felt*) -> (
    public_keys_hash: felt
) {
    if (n_public_keys == 0) {
        return (public_keys_hash=DEFAULT_PUBLIC_KEYS_HASH);
    }
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update(
        hash_state_ptr=hash_state_ptr, data_ptr=public_keys, data_length=n_public_keys
    );
    let (public_keys_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);
    return (public_keys_hash=public_keys_hash);
}
