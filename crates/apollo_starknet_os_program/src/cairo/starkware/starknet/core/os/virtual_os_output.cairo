// Marker indicating SNOS (StarkNet OS) proof facts variant.
const VIRTUAL_SNOS = 'VIRTUAL_SNOS';

// Marker indicating proof facts format version 0.
const PROOF_VERSION = 'PROOF0';

// The version of the virtual OS output.
const VIRTUAL_OS_OUTPUT_VERSION = 'VIRTUAL_SNOS0';

// The header of the proof facts, preceding the virtual OS output.
struct ProofHeader {
    proof_version: felt,
    proof_variant: felt,
    program_hash: felt,
}

// The header of the virtual OS output.
struct VirtualOsOutputHeader {
    output_version: felt,
    // The block number and hash that this run is based on.
    base_block_number: felt,
    base_block_hash: felt,
    starknet_os_config_hash: felt,
    n_messages_to_l1: felt,
}
