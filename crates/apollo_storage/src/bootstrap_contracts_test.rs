use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_types_core::felt::Felt;

use crate::bootstrap_contracts::{
    bootstrap_account_class_hash,
    bootstrap_account_compiled_class_hash,
    bootstrap_account_sierra,
    bootstrap_erc20_class_hash,
    bootstrap_erc20_compiled_class_hash,
    bootstrap_erc20_sierra,
    BOOTSTRAP_ACCOUNT_CLASS_HASH,
    BOOTSTRAP_ACCOUNT_COMPILED_CLASS_HASH,
    BOOTSTRAP_ERC20_CLASS_HASH,
    BOOTSTRAP_ERC20_COMPILED_CLASS_HASH,
};

/// Loads hardcoded Sierra/CASM via the public API (parse + conversion + CASM hash) and asserts
/// stable hashes. Regenerating committed JSON or changing hash rules requires updating these
/// constants deliberately.
#[test]
fn bootstrap_hardcoded_artifacts_load_and_match_expected_hashes() {
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
