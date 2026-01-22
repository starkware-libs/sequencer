// The version of the virtual OS output (short string 'VIRTUAL_SNOS0').
const VIRTUAL_OS_OUTPUT_VERSION = 'VIRTUAL_SNOS0';

// The header of the virtual OS output.
struct VirtualOsOutputHeader {
    version: felt,
    // The block number and hash that this run is based on.
    base_block_number: felt,
    base_block_hash: felt,
    starknet_os_config_hash: felt,
    // The account address that is authorized to run transactions.
    authorized_account_address: felt,
    messages_to_l1_segment_size: felt,
}
