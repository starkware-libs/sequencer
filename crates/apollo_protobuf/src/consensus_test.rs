// ConsensusBlockInfo tests.

use crate::consensus::ConsensusBlockInfo;

#[test]
fn fri_from_wei_converts_correctly() {
    // Conversion rate if 1 ETH = 800 STRK.
    assert_eq!(ConsensusBlockInfo::fri_from_wei(5, 8 * u128::pow(10, 20)), 4000);
}

#[test]
#[should_panic]
fn fri_from_wei_panics_on_gas_too_high() {
    ConsensusBlockInfo::fri_from_wei(u128::pow(2, 127), 4);
}
