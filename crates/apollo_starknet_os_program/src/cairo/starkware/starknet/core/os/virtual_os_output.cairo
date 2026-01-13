// The header of a proof created by the virtual OS.
struct VirtualOsOutputHeader {
    version: felt,
    // The block number and hash that this proof is referencing.
    base_block_number: felt,
    base_block_hash: felt,
    starknet_os_config_hash: felt,
    // The account address that is authorized to use this proof.
    authorized_account_address: felt,
    messages_to_l1_segment_size: felt,
}
