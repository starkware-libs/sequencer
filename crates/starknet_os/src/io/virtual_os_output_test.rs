use starknet_api::block::BlockNumber;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::VIRTUAL_OS_OUTPUT_VERSION;
use starknet_types_core::felt::Felt;

use crate::io::virtual_os_output::VirtualOsOutput;

#[test]
fn test_virtual_os_output_roundtrip() {
    let expected = VirtualOsOutput {
        version: VIRTUAL_OS_OUTPUT_VERSION,
        base_block_number: BlockNumber(42),
        base_block_hash: StarkHash::from(0x1234u64),
        starknet_os_config_hash: StarkHash::from(0x5678u64),
        messages_to_l1_hashes: vec![StarkHash::from(0x9ABCu64), StarkHash::from(0x9ABCu64)],
    };

    let raw_output: Vec<Felt> = vec![
        expected.version,
        Felt::from(expected.base_block_number.0),
        expected.base_block_hash,
        expected.starknet_os_config_hash,
        // Number of messages from l2 to l1.
        Felt::from(expected.messages_to_l1_hashes.len()),
        // The hashes of the messages from l2 to l1.
        expected.messages_to_l1_hashes[0],
        expected.messages_to_l1_hashes[1],
    ];

    let parsed = VirtualOsOutput::from_raw_output(&raw_output).unwrap();

    assert_eq!(parsed, expected);
}
