use assert_matches::assert_matches;
use num_bigint::BigUint;
use rstest::rstest;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Blake2Felt252, Pedersen, StarkHash as CoreStarkHash};

use crate::block::StarknetVersion;
use crate::core::{
    ascii_as_felt,
    calculate_contract_address,
    felt_to_u128,
    AddressDerivationHash,
    ChainId,
    ContractAddress,
    EthAddress,
    Nonce,
    OsChainInfo,
    OsConfigHashVersion,
    PatriciaKey,
    StarknetApiError,
    CONTRACT_ADDRESS_PREFIX,
    L2_ADDRESS_UPPER_BOUND,
    STARKNET_OS_CONFIG_HASH_VERSION_V3,
    STARKNET_OS_CONFIG_HASH_VERSION_V4,
};
use crate::hash::StarkHash;
use crate::transaction::fields::{Calldata, ContractAddressSalt};
use crate::{class_hash, felt, patricia_key};

#[test]
fn patricia_key_valid() {
    let hash = felt!("0x123");
    let patricia_key = PatriciaKey::try_from(hash).unwrap();
    assert_eq!(patricia_key.0, hash);
}

#[test]
fn patricia_key_out_of_range() {
    // 2**251
    let hash = felt!("0x800000000000000000000000000000000000000000000000000000000000000");
    let err = PatriciaKey::try_from(hash);
    assert_matches!(err, Err(StarknetApiError::OutOfRange { string: _err_str }));
}

#[test]
fn patricia_key_macro() {
    assert_eq!(
        patricia_key!("0x123"),
        PatriciaKey::try_from(StarkHash::from_bytes_be(&[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0x1, 0x23
        ]))
        .unwrap()
    );
}

#[test]
fn test_calculate_contract_address() {
    let salt = ContractAddressSalt(Felt::from(1337_u16));
    let class_hash = class_hash!("0x110");
    let deployer_address = ContractAddress::default();
    let constructor_calldata =
        Calldata(vec![Felt::from(60_u16), Felt::from(70_u16), Felt::MAX].into());

    let actual_address = calculate_contract_address(
        salt,
        class_hash,
        &constructor_calldata,
        deployer_address,
        AddressDerivationHash::Pedersen,
    )
    .unwrap();

    let constructor_calldata_hash = Pedersen::hash_array(&constructor_calldata.0);
    let address = Pedersen::hash_array(&[
        Felt::from_hex_unchecked(format!("0x{}", hex::encode(CONTRACT_ADDRESS_PREFIX)).as_str()),
        *deployer_address.0.key(),
        salt.0,
        class_hash.0,
        constructor_calldata_hash,
    ]);
    let (_, mod_address) = address.div_rem(&L2_ADDRESS_UPPER_BOUND);
    let expected_address = ContractAddress::try_from(mod_address).unwrap();

    assert_eq!(actual_address, expected_address);
}

#[test]
fn eth_address_serde() {
    let eth_address = EthAddress::try_from(felt!("0x001")).unwrap();
    let serialized = serde_json::to_string(&eth_address).unwrap();
    assert_eq!(serialized, r#""0x1""#);

    let restored = serde_json::from_str::<EthAddress>(&serialized).unwrap();
    assert_eq!(restored, eth_address);
}

#[test]
fn nonce_overflow() {
    // Increment on this value should overflow back to 0.
    let max_nonce = Nonce(Felt::MAX);

    let overflowed_nonce = max_nonce.try_increment();
    assert_matches!(overflowed_nonce, Err(StarknetApiError::OutOfRange { string: _err_str }));
}

#[test]
fn test_patricia_key_display() {
    assert_eq!(format!("{}", patricia_key!(7_u8)), String::from("0x") + &"0".repeat(63) + "7");
}

#[test]
fn test_contract_address_display() {
    assert_eq!(
        format!("{}", ContractAddress(patricia_key!(16_u8))),
        String::from("0x") + &"0".repeat(62) + "10"
    );
}

#[rstest]
#[case::mainnet(ChainId::Mainnet, "0x534e5f4d41494e")]
#[case::testnet(ChainId::Sepolia, "0x534e5f5345504f4c4941")]
#[case::integration(ChainId::IntegrationSepolia, "0x534e5f494e544547524154494f4e5f5345504f4c4941")]
#[case::other(ChainId::Other("HelloWorld".to_string()), "0x48656c6c6f576f726c64")]
fn test_ascii_as_felt(#[case] chain_id: ChainId, #[case] expected_felt_value: &str) {
    let chain_id_felt = ascii_as_felt(chain_id.to_string().as_str()).unwrap();
    // This is the result of the Python snippet from the Chain-Id documentation.
    let expected_felt = Felt::from_hex_unchecked(expected_felt_value);
    assert_eq!(chain_id_felt, expected_felt);
    assert_eq!(chain_id_felt, Felt::try_from(&chain_id).unwrap())
}

#[test]
fn test_value_too_large_for_type() {
    // Happy flow.
    let n = 1991_u128;
    let n_as_felt = Felt::from(n);
    felt_to_u128(&n_as_felt).unwrap();

    // Value too large for type.
    let overflowed_u128: BigUint = BigUint::from(1_u8) << 128;
    let overflowed_u128_as_felt = Felt::from(overflowed_u128);
    let error = felt_to_u128(&overflowed_u128_as_felt).unwrap_err();
    assert_eq!(
        format!("{error}"),
        "Out of range Felt 340282366920938463463374607431768211456 is too big to convert to \
         'u128'."
    );
}

#[rstest]
// Below the cutover (V0_14_3) the OS config hash stays Pedersen (V3), so pre-cutover blocks remain
// re-executable / re-provable against their original hash.
#[case::pre_cutover(StarknetVersion::V0_14_2, OsConfigHashVersion::V3)]
// At and above the cutover the OS config hash switches to Blake (V4).
#[case::at_cutover(StarknetVersion::V0_14_3, OsConfigHashVersion::V4)]
#[case::latest(StarknetVersion::LATEST, OsConfigHashVersion::V4)]
fn test_os_config_hash_version_gating(
    #[case] starknet_version: StarknetVersion,
    #[case] expected_hash_version: OsConfigHashVersion,
) {
    assert_eq!(OsConfigHashVersion::from(starknet_version), expected_hash_version);

    let chain_info = OsChainInfo {
        chain_id: ChainId::Mainnet,
        strk_fee_token_address: ContractAddress(patricia_key!(0x123_u32)),
    };
    let chain_id_felt: Felt = (&chain_info.chain_id).try_into().unwrap();
    let fee_token_felt: Felt = chain_info.strk_fee_token_address.into();

    let actual_hash = chain_info.compute_os_config_hash(None, starknet_version).unwrap();
    let expected_hash = match expected_hash_version {
        OsConfigHashVersion::V3 => Pedersen::hash_array(&[
            STARKNET_OS_CONFIG_HASH_VERSION_V3,
            chain_id_felt,
            fee_token_felt,
        ]),
        OsConfigHashVersion::V4 => Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&[
            STARKNET_OS_CONFIG_HASH_VERSION_V4,
            chain_id_felt,
            fee_token_felt,
        ]),
    };
    assert_eq!(actual_hash, expected_hash);
}

#[test]
fn test_os_config_hash_pedersen_and_blake_differ() {
    // The same chain config must hash differently before and after the cutover, so a Blake hash is
    // never mistaken for the pre-cutover Pedersen hash that L1 / historical proofs were bound to.
    let chain_info = OsChainInfo {
        chain_id: ChainId::Mainnet,
        strk_fee_token_address: ContractAddress(patricia_key!(0x123_u32)),
    };
    let pedersen_hash = chain_info.compute_os_config_hash(None, StarknetVersion::V0_14_2).unwrap();
    let blake_hash = chain_info.compute_os_config_hash(None, StarknetVersion::V0_14_3).unwrap();
    assert_ne!(pedersen_hash, blake_hash);
}
