from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.hash_state import hash_finalize, hash_init, hash_update_single
from starkware.cairo.common.registers import get_fp_and_pc

const STARKNET_OS_CONFIG_VERSION = 'StarknetOsConfig3';
// The pedersen hash of 0, 0.
const DEFAULT_PUBLIC_KEY_HASH = 0x5d2a2613bcb66b00b159c4ac69e0ed00633e8ca159bf54fe13d356828d2cc13;

struct StarknetOsConfig {
    // The identifier of the chain.
    // This field can be used to prevent replay of testnet transactions on mainnet.
    chain_id: felt,
    // The (L2) address of the fee token contract.
    fee_token_address: felt,
    // The hash of POTC's public key. Default key is 0, 0 for SN environment.
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

func get_public_key_hash{hash_ptr: HashBuiltin*}(public_key: EcPoint*) -> (public_key_hash: felt) {
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=public_key.x);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=public_key.y);
    let (public_key_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);
    return (public_key_hash=public_key_hash);
}
