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
    "0x0462b054af23d1f3b9da196a296ccdebfbabadee501bfb76e1c573cb93487abd";
const BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH: &str =
    "0x1a4828d73b49e6ec515d2c879a5a1b2870439c83c81517e40973d8f2d11b1a7";
const BOOTSTRAP_ERC20_COMPILED_CLASS_HASH: &str =
    "0x7352cd4c7c86d16bb9dbe28d286f78279e27017f731f8afe1562dede8a41cb3";

/// Loads Sierra/CASM via the public API (parse + conversion + CASM hash) and asserts stable
/// hashes. Regenerating committed JSON or changing hash rules requires updating these constants
/// deliberately.
#[test]
fn bootstrap_artifacts_load_and_match_expected_hashes() {
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
