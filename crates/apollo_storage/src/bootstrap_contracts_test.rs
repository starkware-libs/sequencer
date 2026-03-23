use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use crate::bootstrap_contracts::{
    bootstrap_account_class_hash,
    bootstrap_account_compiled_class_hash,
    bootstrap_account_sierra,
    bootstrap_erc20_class_hash,
    bootstrap_erc20_compiled_class_hash,
    bootstrap_erc20_sierra,
};

const BOOTSTRAP_ACCOUNT_CLASS_HASH: &str =
    "0x23f6d63bd54a867e571beb1f98b5461f7f58b7647c01b2b4fb4b00c157bc709";
const BOOTSTRAP_ERC20_CLASS_HASH: &str =
    "0x7ffc2b4185362a4d10b3878730b979e475739d9c5fd1d698c08c94c58cf1021";
const BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH: &str =
    "0x1a4828d73b49e6ec515d2c879a5a1b2870439c83c81517e40973d8f2d11b1a7";
const BOOTSTRAP_ERC20_COMPILED_CLASS_HASH: &str =
    "0x3efdb18d62e7470738b2fd03c285eb2c164f5175290d28b6a516779ca674514";

/// Loads embedded Sierra/CASM via the public API (parse + conversion + CASM hash) and asserts
/// stable hashes. Regenerating committed JSON or changing hash rules requires updating these
/// constants deliberately.
#[test]
fn bootstrap_embedded_artifacts_load_and_match_expected_hashes() {
    let account_sierra = bootstrap_account_sierra();
    let erc20_sierra = bootstrap_erc20_sierra();
    assert!(!account_sierra.sierra_program.is_empty());
    assert!(!erc20_sierra.sierra_program.is_empty());

    assert_eq!(
        bootstrap_account_class_hash(),
        ClassHash(Felt::from_hex(BOOTSTRAP_ACCOUNT_CLASS_HASH).unwrap()),
    );
    assert_eq!(
        bootstrap_erc20_class_hash(),
        ClassHash(Felt::from_hex(BOOTSTRAP_ERC20_CLASS_HASH).unwrap()),
    );
    assert_eq!(
        bootstrap_account_compiled_class_hash(),
        CompiledClassHash(Felt::from_hex(BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH).unwrap()),
    );
    assert_eq!(
        bootstrap_erc20_compiled_class_hash(),
        CompiledClassHash(Felt::from_hex(BOOTSTRAP_ERC20_COMPILED_CLASS_HASH).unwrap()),
    );
}
