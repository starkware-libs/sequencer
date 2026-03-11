// Marker indicating SNOS (StarkNet OS) proof facts variant.
const VIRTUAL_SNOS = 'VIRTUAL_SNOS';

// Marker indicating proof facts format version 0.
const PROOF_VERSION = 'PROOF0';

// The version of the virtual OS output.
//
// === VIRTUAL_SNOS0 Version Contract ===
//
// This version string is a commitment to the exact behavior and output format of the virtual OS.
// Any change to the following guarantees MUST be accompanied by a version bump.
//
// 1. Output format:
//    The output is a flat array of felts with the following layout:
//      [output_version, base_block_number, base_block_hash, starknet_os_config_hash,
//       n_l2_to_l1_messages, message_hash_0, message_hash_1, ...]
//    - output_version: the VIRTUAL_OS_OUTPUT_VERSION constant.
//    - base_block_number / base_block_hash: the block this run is based on. The hash is
//      computed (proven) by the OS from the block info and the initial state root.
//    - starknet_os_config_hash: Poseidon hash of the Starknet OS config.
//    - n_l2_to_l1_messages: count of L2-to-L1 message hashes that follow.
//    - Each message hash is Blake2s over the naive encoding (8 u32 LE limbs per felt) of
//      [from_address, to_address, payload_size, ...payload].
//    No state diff, data availability, or state roots are included.
//
// 2. Single block, single transaction:
//    - Exactly 1 block is processed (asserted in process_os_output).
//    - Exactly 1 transaction per block, which must be INVOKE_FUNCTION (asserted in
//      execute_transactions_inner).
//
// 3. Blocked syscalls:
//    The following syscalls are NOT available in virtual OS mode and will cause a Cairo error:
//      - Deploy
//      - GetBlockHash
//      - ReplaceClass
//      - Keccak
//      - MetaTxV0
//
// 4. Cairo 1 only:
//    Only Sierra (Cairo 1) contracts are supported. Deprecated (Cairo 0) entry points are
//    unreachable.
//
// 5. No proof facts:
//    The virtual OS does not support recursive proof facts (proof_facts_size must be 0).
//
// 6. Block info semantics:
//     get_execution_info returns the **base (previous) block** info.
//
// Changes to ANY of the above MUST trigger a version bump.
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
    n_l2_to_l1_messages: felt,
}
