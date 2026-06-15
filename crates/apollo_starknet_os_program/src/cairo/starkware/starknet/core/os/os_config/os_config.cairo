from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.hash_state import hash_finalize as pedersen_hash_finalize
from starkware.cairo.common.hash_state import hash_init as pedersen_hash_init
from starkware.cairo.common.hash_state import hash_update as pedersen_hash_update
from starkware.starknet.core.os.hash.hash_state_blake import (
    HashState,
    hash_finalize,
    hash_init,
    hash_update_single,
)

const STARKNET_OS_CONFIG_VERSION = 'StarknetOsConfig4';

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
// The top-level hash uses Blake; the public_keys_hash field is itself a Pedersen digest (see
// get_public_keys_hash) and is absorbed here as a single felt.
func get_starknet_os_config_hash{range_check_ptr}(starknet_os_config: StarknetOsConfig*) -> (
    starknet_os_config_hash: felt
) {
    static_assert StarknetOsConfig.SIZE == 3;
    let hash_state: HashState = hash_init();
    with hash_state {
        hash_update_single(item=STARKNET_OS_CONFIG_VERSION);
        hash_update_single(item=starknet_os_config.chain_id);
        hash_update_single(item=starknet_os_config.fee_token_address);
        // If in the future another optional field is added to StarknetOsConfig,
        // remove the following `if`.
        if (starknet_os_config.public_keys_hash != DEFAULT_PUBLIC_KEYS_HASH) {
            hash_update_single(item=starknet_os_config.public_keys_hash);
            tempvar hash_state = hash_state;
        } else {
            tempvar hash_state = hash_state;
        }
    }
    let starknet_os_config_hash: felt = hash_finalize(hash_state=hash_state);

    return (starknet_os_config_hash=starknet_os_config_hash);
}

// Computes the hash of the public keys, returns 0 if there are no public keys.
// Note: the public keys hash uses the Pedersen hash, regardless of the OS config hash version.
func get_public_keys_hash{hash_ptr: HashBuiltin*}(n_public_keys: felt, public_keys: felt*) -> (
    public_keys_hash: felt
) {
    if (n_public_keys == 0) {
        return (public_keys_hash=DEFAULT_PUBLIC_KEYS_HASH);
    }
    let (hash_state_ptr) = pedersen_hash_init();
    let (hash_state_ptr) = pedersen_hash_update(
        hash_state_ptr=hash_state_ptr, data_ptr=public_keys, data_length=n_public_keys
    );
    let (public_keys_hash) = pedersen_hash_finalize(hash_state_ptr=hash_state_ptr);
    return (public_keys_hash=public_keys_hash);
}
