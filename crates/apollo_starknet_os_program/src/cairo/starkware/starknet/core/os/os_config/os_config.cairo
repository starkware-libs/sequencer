from starkware.cairo.common.cairo_blake2s.blake2s import encode_felt252_data_and_calc_blake_hash
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
// The top-level hash uses Blake; the public_keys_hash field is itself a Blake digest (see
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
func get_public_keys_hash{range_check_ptr}(n_public_keys: felt, public_keys: felt*) -> (
    public_keys_hash: felt
) {
    if (n_public_keys == 0) {
        return (public_keys_hash=DEFAULT_PUBLIC_KEYS_HASH);
    }
    let (public_keys_hash) = encode_felt252_data_and_calc_blake_hash(
        data_len=n_public_keys, data=public_keys
    );
    return (public_keys_hash=public_keys_hash);
}
