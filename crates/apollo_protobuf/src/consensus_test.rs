// ConsensusBlockInfo tests.

use starknet_api::block::GasPrice;

use crate::consensus::ConsensusBlockInfo;

#[test]
fn wei_to_fri_converts_correctly() {
    // Conversion rate if 1 ETH = 800 STRK.
    assert_eq!(ConsensusBlockInfo::wei_to_fri(GasPrice(5), 8 * u128::pow(10, 20)).0, 4000);
}

#[test]
#[should_panic]
fn wei_to_fri_panics_on_gas_too_high() {
    ConsensusBlockInfo::wei_to_fri(GasPrice(u128::pow(2, 127)), 4);
}
