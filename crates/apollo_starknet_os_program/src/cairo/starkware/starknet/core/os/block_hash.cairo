from starkware.cairo.common.cairo_builtins import PoseidonBuiltin
from starkware.cairo.common.hash_state_poseidon import (
    HashState,
    hash_finalize,
    hash_init,
    hash_update_single,
)
from starkware.starknet.common.new_syscalls import BlockInfo

// The latest block hash version.
const BLOCK_HASH_VERSION = 'STARKNET_BLOCK_HASH1';

struct BlockHeaderCommitments {
    transaction_commitment: felt,
    event_commitment: felt,
    receipt_commitment: felt,
    state_diff_commitment: felt,
    concatenated_counts: felt,
}

// Calculates the block hash given the top level components.
func calculate_block_hash{poseidon_ptr: PoseidonBuiltin*}(
    block_info: BlockInfo*,
    header_commitments: BlockHeaderCommitments*,
    gas_prices_hash: felt,
    state_root: felt,
    parent_hash: felt,
    starknet_version: felt,
) -> felt {
    let hash_state = hash_init();
    with hash_state {
        hash_update_single(BLOCK_HASH_VERSION);
        hash_update_single(block_info.block_number);
        hash_update_single(state_root);
        hash_update_single(block_info.sequencer_address);
        hash_update_single(block_info.block_timestamp);
        hash_update_single(header_commitments.concatenated_counts);
        hash_update_single(header_commitments.state_diff_commitment);
        hash_update_single(header_commitments.transaction_commitment);
        hash_update_single(header_commitments.event_commitment);
        hash_update_single(header_commitments.receipt_commitment);
        hash_update_single(gas_prices_hash);
        hash_update_single(starknet_version);
        hash_update_single(0);
        hash_update_single(parent_hash);
    }

    let block_hash = hash_finalize(hash_state=hash_state);
    return block_hash;
}
