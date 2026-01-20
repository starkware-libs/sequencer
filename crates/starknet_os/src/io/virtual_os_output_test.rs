use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_types_core::felt::Felt;

use crate::io::virtual_os_output::VirtualOsOutput;

#[test]
fn test_virtual_os_output_roundtrip() {
    let expected = VirtualOsOutput {
        version: Felt::from(0u64),
        base_block_number: BlockNumber(42),
        base_block_hash: StarkHash::from(0x1234u64),
        starknet_os_config_hash: StarkHash::from(0x5678u64),
        authorized_account_address: ContractAddress::from(0x9ABCu64),
        messages_to_l1_hash: StarkHash::from(0x9ABCu64),
    };

    let raw_output: Vec<Felt> = vec![
        expected.version,
        Felt::from(expected.base_block_number.0),
        expected.base_block_hash,
        expected.starknet_os_config_hash,
        *expected.authorized_account_address.0.key(),
        expected.messages_to_l1_hash,
    ];

    let parsed = VirtualOsOutput::from_raw_output(&raw_output).unwrap();

    assert_eq!(parsed, expected);
}
