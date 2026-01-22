// The header of the virtual OS output.
struct VirtualOsOutputHeader {
    version: felt,
    // The block number and hash that this run is based on.
    base_block_number: felt,
    base_block_hash: felt,
    starknet_os_config_hash: felt,
    messages_to_l1_segment_size: felt,
}
