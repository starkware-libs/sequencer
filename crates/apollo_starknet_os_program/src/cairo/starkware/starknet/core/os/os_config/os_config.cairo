from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.hash_state import (
    hash_finalize,
    hash_init,
    hash_update,
    hash_update_single,
)
from starkware.cairo.common.registers import get_fp_and_pc

const STARKNET_OS_CONFIG_VERSION = 'StarknetOsConfig3';

const DEFAULT_PUBLIC_KEY_HASH = 0;

struct StarknetOsConfig {
    // The identifier of the chain.
    // This field can be used to prevent replay of testnet transactions on mainnet.
    chain_id: felt,
    // The (L2) address of the fee token contract.
    fee_token_address: felt,
    // The hash of the public key used to decrypt the state diff.
    // The default hash is 0, as used in Starknet environments.
    public_key_hash: felt,
}

// Calculates the hash of StarkNet OS config.
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
    if (starknet_os_config.public_key_hash != DEFAULT_PUBLIC_KEY_HASH) {
        let (hash_state_ptr) = hash_update_single(
            hash_state_ptr=hash_state_ptr, item=starknet_os_config.public_key_hash
        );
    } else {
        // align the stack.
        tempvar hash_ptr = hash_ptr;
        tempvar hash_state_ptr = hash_state_ptr;
    }
    let (starknet_os_config_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);

    return (starknet_os_config_hash=starknet_os_config_hash);
}

func get_public_key_hash{hash_ptr: HashBuiltin*}(public_keys_start: felt*, n_keys: felt) -> (
    public_key_hash: felt
) {
    if (n_keys == 0) {
        return (public_key_hash=0);
    }
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update(
        hash_state_ptr=hash_state_ptr, data_ptr=public_keys_start, data_length=n_keys
    );
    let (public_key_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);
    return (public_key_hash=public_key_hash);
}
