struct VirtualOsOutputHeader {
    version: felt,
    prev_block_number: felt,
    prev_block_hash: felt,
    starknet_os_config_hash: felt,
    authorized_account_address: felt,
    messages_to_l1_segment_size: felt,
}
