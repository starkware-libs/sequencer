use starknet_types_core::short_string::ShortString;
use strum::IntoEnumIterator;

use super::*;
use crate::block::{GasPrice, NonzeroGasPrice};

/// Returns SNOS-shaped `ProofFacts` whose first felt is the given proof version.
fn proof_facts_given_proof_version(proof_version: Felt) -> ProofFacts {
    let mut facts = ProofFacts::snos_proof_facts_for_testing();
    Arc::make_mut(&mut facts.0)[0] = proof_version;
    facts
}

/// `checked_div_ceil` panics (debug) or returns wrong value (release) when
/// floor(fee / price) == u64::MAX but a remainder still exists, so the ceiling
/// would need GasAmount(u64::MAX + 1) which overflows the u64 field.
///
/// Specifically, the buggy line `(value.0 + 1).into()` at
/// crates/starknet_api/src/transaction/fields.rs performs an unchecked u64
/// addition on `value.0` without checking for overflow first.
///
/// After the fix, `checked_div_ceil` should return `None` in this case.
#[test]
fn checked_div_ceil_returns_none_when_ceiling_overflows_gas_amount() {
    // fee = u64::MAX * 2 + 1 = 2^65 - 1 (fits in u128).
    let fee = Fee((u64::MAX as u128) * 2 + 1);
    // price = 2, so floor(fee / price) = u64::MAX (fits in u64).
    let price = NonzeroGasPrice::new(GasPrice(2)).unwrap();

    // floor = u64::MAX, and u64::MAX * 2 = fee - 1 < fee, so a remainder exists.
    // The true ceiling is u64::MAX + 1 which overflows GasAmount (u64).
    // checked_div_ceil must therefore return None instead of overflowing.
    assert_eq!(
        fee.checked_div_ceil(price),
        None,
        "ceiling of fee / price overflows GasAmount; checked_div_ceil must return None"
    );
}

#[test]
fn proof_facts_variant_accepts_supported_versions() {
    for version in ProofVersion::iter() {
        let variant =
            ProofFactsVariant::try_from(&proof_facts_given_proof_version(version.as_felt()))
                .expect("supported version should parse");
        match variant {
            ProofFactsVariant::Snos(snos) => assert_eq!(snos.proof_version, version),
            ProofFactsVariant::Empty => panic!("expected Snos variant"),
        }
    }
}

#[test]
fn proof_facts_variant_rejects_unknown_version() {
    let facts = proof_facts_given_proof_version(Felt::from_hex_unchecked("0xDEAD"));
    assert!(matches!(
        ProofFactsVariant::try_from(&facts),
        Err(StarknetApiError::InvalidProofFacts(_))
    ));
}

#[test]
fn proof_version_str_encodes_to_felt() {
    for version in ProofVersion::iter() {
        let from_short_string =
            Felt::from(ShortString::try_from(version.as_str()).expect("valid short string"));
        assert_eq!(from_short_string, version.as_felt());
    }
}
