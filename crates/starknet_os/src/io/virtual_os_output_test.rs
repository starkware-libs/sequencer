use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::VIRTUAL_OS_OUTPUT_VERSION;
use starknet_types_core::felt::Felt;

use crate::io::virtual_os_output::VirtualOsOutput;

#[test]
fn test_virtual_os_output_roundtrip() {
    let expected = VirtualOsOutput {
        version: Felt::from(VIRTUAL_OS_OUTPUT_VERSION),
        base_block_number: BlockNumber(42),
        base_block_hash: StarkHash::from(0x1234u64),
        starknet_os_config_hash: StarkHash::from(0x5678u64),
        authorized_account_address: ContractAddress::from(0x9ABCu64),
        messages_to_l1: vec![],
    };

    let raw_output: Vec<Felt> = vec![
        expected.version,
        Felt::from(expected.base_block_number.0),
        expected.base_block_hash,
        expected.starknet_os_config_hash,
        *expected.authorized_account_address.0.key(),
        Felt::ZERO, // messages_to_l1_segment_size = 0
    ];

    let parsed = VirtualOsOutput::from_raw_output(&raw_output).unwrap();

    assert_eq!(parsed, expected);
}
