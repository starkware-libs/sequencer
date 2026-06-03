use rstest::rstest;
use starknet_api::core::EthAddress;
use starknet_api::hash::StarkHash;

use crate::eip55::eip55_checksum_address;

/// Every case is a ground-truth value served by the live Python feeder gateway's
/// get_contract_addresses on mainnet or sepolia (captured 2026-06-03).
#[rstest]
#[case::mainnet_starknet("0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4")]
#[case::mainnet_gps_statement_verifier("0x47312450B3Ac8b5b8e247a6bB6d523e7605bDb60")]
#[case::sepolia_starknet("0xE2Bb56ee936fd6433DC0F6e7e3b8365C906AA057")]
#[case::sepolia_gps_statement_verifier("0xf294781D719D2F4169cE54469C28908E6FA752C1")]
#[case::sepolia_memory_page_fact_registry("0x5628E75245Cc69eCA0994F0449F4dDA9FbB5Ec6a")]
#[case::sepolia_merkle_statement_contract("0xd414f8f535D4a96cB00fFC8E85160b353cb7809c")]
#[case::sepolia_fri_statement_contract("0x55d049b4C82807808E76e61a08C6764bbf2ffB55")]
#[case::sepolia_hybrid_gps_fact_adapter("0x68cb84164E27cbf65222F604BAef58CC4149FCFC")]
fn checksum_matches_live_feeder_gateway(#[case] live_checksummed_address: &str) {
    let address = EthAddress::try_from(
        StarkHash::from_hex(&live_checksummed_address.to_lowercase()).unwrap(),
    )
    .unwrap();
    assert_eq!(eip55_checksum_address(&address), live_checksummed_address);
}
